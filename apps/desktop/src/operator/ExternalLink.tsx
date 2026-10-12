import type { ReactNode } from "react";
import { openExternal, parseExternalUrl } from "../lib/external";

/**
 * Link to a web page, opened in the system browser (never in this webview). A URL that is not
 * absolute https is shown as plain text instead of as a link.
 */
export function ExternalLink({ href, children }: { href: string; children?: ReactNode }) {
  const url = parseExternalUrl(href);
  if (!url) return <>{children ?? href}</>;
  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      onClick={(e) => {
        e.preventDefault();
        void openExternal(url);
      }}
    >
      {children ?? url}
    </a>
  );
}
