/** Visual class of a license badge: public domain / free, attribution required, restricted. */
export type LicenseTone = "free" | "attrib" | "restricted";

export function licenseTone(license: string): LicenseTone {
  const l = license.toUpperCase();
  if (l.startsWith("CC0") || l.startsWith("PDM")) return "free";
  if (l.includes("-NC") || l.includes("-ND") || l === "OTRA") return "restricted";
  return "attrib";
}

export function formatBytes(n: number): string {
  if (n >= 1_048_576) return `${(n / 1_048_576).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(n / 1024))} KB`;
}
