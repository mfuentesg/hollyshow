import qrcode from "qrcode-generator";
import { useMemo } from "react";

/** QR code rendered as inline SVG (no innerHTML). Dark modules on a white quiet zone. */
export function QrCode({
  value,
  size = 180,
  label,
}: {
  value: string;
  size?: number;
  label: string;
}) {
  const modules = useMemo(() => {
    const qr = qrcode(0, "M");
    qr.addData(value);
    qr.make();
    const n = qr.getModuleCount();
    const rects: string[] = [];
    for (let r = 0; r < n; r++) {
      let start = -1;
      for (let c = 0; c <= n; c++) {
        const dark = c < n && qr.isDark(r, c);
        if (dark && start < 0) start = c;
        if (!dark && start >= 0) {
          rects.push(`M${start} ${r}h${c - start}v1h-${c - start}z`);
          start = -1;
        }
      }
    }
    return { n, d: rects.join("") };
  }, [value]);
  const quiet = 3;
  const total = modules.n + quiet * 2;
  return (
    <svg
      role="img"
      aria-label={label}
      width={size}
      height={size}
      viewBox={`${-quiet} ${-quiet} ${total} ${total}`}
      shapeRendering="crispEdges"
      className="qr"
    >
      <rect x={-quiet} y={-quiet} width={total} height={total} fill="#fff" />
      <path d={modules.d} fill="#000" />
    </svg>
  );
}
