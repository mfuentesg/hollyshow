import { describe, expect, it, vi } from "vitest";
import { isExternalUrl, openExternal, parseExternalUrl } from "./external";

describe("external links", () => {
  it("accepts only absolute https URLs", () => {
    expect(parseExternalUrl("https://example.com/a?b=1#c")).toBe("https://example.com/a?b=1#c");
    expect(parseExternalUrl("  https://example.com ")).toBe("https://example.com/");
    for (const bad of [
      "http://example.com",
      "file:///etc/passwd",
      "javascript:alert(1)",
      "data:text/html,hi",
      "mailto:a@b.c",
      "hollyshow://update",
      "//example.com",
      "/relative",
      "https://",
      "https://user:pw@example.com",
      "",
      null,
      undefined,
      42,
    ]) {
      expect(isExternalUrl(bad), String(bad)).toBe(false);
    }
  });

  it("opens through the opener with the normalized URL", async () => {
    const opener = vi.fn().mockResolvedValue(undefined);
    await expect(openExternal("https://example.com", opener)).resolves.toBe(true);
    expect(opener).toHaveBeenCalledWith("https://example.com/");
  });

  it("rejects non-https without calling the opener", async () => {
    const opener = vi.fn().mockResolvedValue(undefined);
    for (const bad of ["http://example.com", "file:///x", "javascript:1", "custom://x"]) {
      await expect(openExternal(bad, opener)).resolves.toBe(false);
    }
    expect(opener).not.toHaveBeenCalled();
  });

  it("reports opener failures instead of throwing", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const opener = vi.fn().mockRejectedValue(new Error("denied"));
    await expect(openExternal("https://example.com", opener)).resolves.toBe(false);
    warn.mockRestore();
  });
});
