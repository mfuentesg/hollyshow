/**
 * True for acks rejecting a relative command that was issued against stale state (another
 * version, or another host boot epoch). The state resync that follows repairs these, so UIs
 * must not toast them.
 */
export function isStaleCommandCode(code: string | undefined | null): boolean {
  return code === "version_mismatch" || code === "epoch_mismatch";
}
