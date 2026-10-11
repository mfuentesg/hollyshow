/**
 * Optional WebGL transition path for the background layer (PLAN §2.2 "transiciones avanzadas").
 * Three fragment shaders blend two textures. Everything degrades: when WebGL is missing, the
 * context is lost, or a shader fails to compile, callers fall back to the DOM crossfade.
 */

export type ShaderName = "dissolve" | "wipe" | "ripple";
export const SHADER_NAMES: ShaderName[] = ["dissolve", "wipe", "ripple"];

const VERTEX = `
attribute vec2 a_pos;
varying vec2 v_uv;
void main() {
  v_uv = a_pos * 0.5 + 0.5;
  v_uv.y = 1.0 - v_uv.y;
  gl_Position = vec4(a_pos, 0.0, 1.0);
}`;

const HEADER = `
precision mediump float;
varying vec2 v_uv;
uniform sampler2D u_from;
uniform sampler2D u_to;
uniform float u_progress;
float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
             mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}`;

/** Noisy dissolve with a soft edge: the new image grows from random blobs. */
const DISSOLVE = `${HEADER}
void main() {
  float n = noise(v_uv * 6.0) * 0.6 + noise(v_uv * 14.0) * 0.4;
  float edge = 0.18;
  float t = u_progress * (1.0 + edge) - edge;
  float m = smoothstep(t, t + edge, 1.0 - n);
  gl_FragColor = mix(texture2D(u_to, v_uv), texture2D(u_from, v_uv), m);
}`;

/** Soft diagonal wipe from the top-left corner. */
const WIPE = `${HEADER}
void main() {
  float d = (v_uv.x + v_uv.y) * 0.5;
  float edge = 0.12;
  float t = u_progress * (1.0 + edge) - edge;
  float m = smoothstep(t, t + edge, d);
  gl_FragColor = mix(texture2D(u_to, v_uv), texture2D(u_from, v_uv), m);
}`;

/** Water-like displacement that settles while cross-dissolving. */
const RIPPLE = `${HEADER}
void main() {
  vec2 c = v_uv - 0.5;
  float r = length(c);
  float amp = sin(u_progress * 3.14159) * 0.03;
  vec2 off = normalize(c + 0.0001) * sin(r * 40.0 - u_progress * 12.0) * amp;
  vec4 a = texture2D(u_from, v_uv + off);
  vec4 b = texture2D(u_to, v_uv - off);
  gl_FragColor = mix(a, b, smoothstep(0.0, 1.0, u_progress));
}`;

export const SHADER_SOURCES: Record<ShaderName, string> = {
  dissolve: DISSOLVE,
  wipe: WIPE,
  ripple: RIPPLE,
};

export const VERTEX_SOURCE = VERTEX;

/** Picks a shader deterministically from a seed (slide id) so reruns look the same. */
export function pickShader(seed: string): ShaderName {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return SHADER_NAMES[h % SHADER_NAMES.length] ?? "dissolve";
}

let support: boolean | undefined;

/** Feature detection, memoised. Also false on the server and in test environments. */
export function detectWebGL(): boolean {
  if (support !== undefined) return support;
  try {
    if (typeof document === "undefined") return (support = false);
    const c = document.createElement("canvas");
    support = Boolean(c.getContext("webgl") ?? c.getContext("experimental-webgl"));
  } catch {
    support = false;
  }
  return support;
}

/** Test hook. */
export function resetWebGLDetection(value?: boolean): void {
  support = value;
}

type Source = TexImageSource;

/**
 * Plays one shader transition between two textures on a canvas. Throws from the constructor when
 * WebGL is unavailable or a program fails, so callers can fall back with a single try/catch.
 */
export class ShaderTransition {
  private gl: WebGLRenderingContext;
  private programs = new Map<ShaderName, WebGLProgram>();
  private textures: [WebGLTexture, WebGLTexture];
  private buffer: WebGLBuffer;
  private raf = 0;
  private disposed = false;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl", { premultipliedAlpha: false, antialias: false });
    if (!gl) throw new Error("webgl unavailable");
    this.gl = gl;
    const buffer = gl.createBuffer();
    const t0 = gl.createTexture();
    const t1 = gl.createTexture();
    if (!buffer || !t0 || !t1) throw new Error("webgl resources");
    this.buffer = buffer;
    this.textures = [t0, t1];
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    for (const t of this.textures) {
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    }
  }

  private program(name: ShaderName): WebGLProgram {
    const cached = this.programs.get(name);
    if (cached) return cached;
    const gl = this.gl;
    const compile = (type: number, src: string) => {
      const s = gl.createShader(type);
      if (!s) throw new Error("shader alloc");
      gl.shaderSource(s, src);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
        throw new Error(gl.getShaderInfoLog(s) ?? "shader compile");
      }
      return s;
    };
    const p = gl.createProgram();
    if (!p) throw new Error("program alloc");
    gl.attachShader(p, compile(gl.VERTEX_SHADER, VERTEX));
    gl.attachShader(p, compile(gl.FRAGMENT_SHADER, SHADER_SOURCES[name]));
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error("program link");
    this.programs.set(name, p);
    return p;
  }

  private upload(index: 0 | 1, src: Source): void {
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0 + index);
    gl.bindTexture(gl.TEXTURE_2D, this.textures[index]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, src);
  }

  private draw(name: ShaderName, progress: number): void {
    const gl = this.gl;
    const prog = this.program(name);
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.useProgram(prog);
    const loc = gl.getAttribLocation(prog, "a_pos");
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    gl.uniform1i(gl.getUniformLocation(prog, "u_from"), 0);
    gl.uniform1i(gl.getUniformLocation(prog, "u_to"), 1);
    gl.uniform1f(gl.getUniformLocation(prog, "u_progress"), progress);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  /** Runs the transition; resolves at the end (or immediately when disposed). */
  run(name: ShaderName, from: Source, to: Source, durationMs: number): Promise<void> {
    this.upload(0, from);
    this.upload(1, to);
    return new Promise((resolve) => {
      const start = performance.now();
      const frame = (now: number) => {
        if (this.disposed) return resolve();
        const t = Math.min(1, (now - start) / Math.max(1, durationMs));
        try {
          this.draw(name, t);
        } catch {
          return resolve();
        }
        if (t < 1) this.raf = requestAnimationFrame(frame);
        else resolve();
      };
      this.raf = requestAnimationFrame(frame);
    });
  }

  dispose(): void {
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    const gl = this.gl;
    this.programs.forEach((p) => gl.deleteProgram(p));
    this.textures.forEach((t) => gl.deleteTexture(t));
    gl.deleteBuffer(this.buffer);
    gl.getExtension("WEBGL_lose_context")?.loseContext();
  }
}
