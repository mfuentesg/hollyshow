import { licenseTone } from "./license";

export function LicenseBadge({ license, title }: { license: string; title?: string }) {
  return (
    <span
      className={`license-badge license-badge--${licenseTone(license)}`}
      title={title ?? license}
    >
      {license}
    </span>
  );
}
