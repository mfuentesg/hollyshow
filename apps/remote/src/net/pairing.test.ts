import { describe, expect, it, vi } from "vitest";
import { PairError, pair, readLaunchParams, serverOrigin } from "./pairing";

describe("readLaunchParams", () => {
  it("reads the QR token", () => {
    expect(readLaunchParams("?pair=abcDEF123456-_", "")).toEqual({
      pairToken: "abcDEF123456-_",
      congregation: false,
    });
  });
  it("rejects malformed tokens", () => {
    expect(readLaunchParams("?pair=<script>", "").pairToken).toBeNull();
    expect(readLaunchParams("?pair=ab", "").pairToken).toBeNull();
  });
  it("detects the public view", () => {
    expect(readLaunchParams("?view=congregation", "").congregation).toBe(true);
    expect(readLaunchParams("?c", "").congregation).toBe(true);
    expect(readLaunchParams("", "#/congregacion").congregation).toBe(true);
    expect(readLaunchParams("", "").congregation).toBe(false);
  });
});

describe("serverOrigin", () => {
  it("derives http and ws from the page", () => {
    expect(serverOrigin({ protocol: "http:", host: "192.168.1.5:7420", search: "" })).toEqual({
      http: "http://192.168.1.5:7420",
      ws: "ws://192.168.1.5:7420/ws",
    });
    expect(serverOrigin({ protocol: "https:", host: "x.local", search: "" }).ws).toBe(
      "wss://x.local/ws",
    );
  });
  it("ignores the host override in production builds", () => {
    const loc = { protocol: "http:", host: "192.168.1.5:7420", search: "?host=evil.example:80" };
    expect(serverOrigin(loc, false).http).toBe("http://192.168.1.5:7420");
    expect(serverOrigin(loc, false).ws).toBe("ws://192.168.1.5:7420/ws");
  });
  it("allows a dev override but not arbitrary strings", () => {
    expect(
      serverOrigin(
        { protocol: "http:", host: "localhost:5174", search: "?host=192.168.1.5:7420" },
        true,
      ).ws,
    ).toBe("ws://192.168.1.5:7420/ws");
    expect(serverOrigin({ protocol: "http:", host: "a", search: "?host=evil.com/x?" }).http).toBe(
      "http://a",
    );
  });
});

describe("pair", () => {
  const reply = (status: number, body: unknown) =>
    vi
      .fn()
      .mockResolvedValue({ ok: status >= 200 && status < 300, status, json: async () => body });

  it("posts the PIN and returns credentials", async () => {
    const f = reply(200, { token: "T", deviceId: "d", role: "pastor" });
    await expect(pair("http://h", { pin: "123456" }, "Pixel", f)).resolves.toEqual({
      token: "T",
      role: "pastor",
    });
    expect(f).toHaveBeenCalledWith(
      "http://h/api/pair",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ pin: "123456", deviceName: "Pixel" }),
      }),
    );
  });
  it("maps failures", async () => {
    await expect(
      pair("http://h", { pin: "1" }, "x", reply(401, { message: "PIN" })),
    ).rejects.toMatchObject({
      failure: "invalid",
    });
    await expect(
      pair(
        "http://h",
        { pin: "1" },
        "x",
        reply(429, { message: "demasiados intentos; reintenta en 30 s" }),
      ),
    ).rejects.toBeInstanceOf(PairError);
    await expect(
      pair("http://h", { pin: "1" }, "x", vi.fn().mockRejectedValue(new Error("net"))),
    ).rejects.toMatchObject({
      failure: "network",
    });
    await expect(pair("http://h", { pin: "1" }, "x", reply(200, {}))).rejects.toMatchObject({
      failure: "bad_request",
    });
  });
});
