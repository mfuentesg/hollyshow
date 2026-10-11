export * from "./generated";
export * from "./api";
export { commands } from "./commands";
export { createMockApi } from "./mock";
export type { MockSeed } from "./mock";
export { mockApply } from "./reducer";
export { emptyState, lyricSlide, sampleShow } from "./fixtures";
/** Must equal `PROTOCOL_VERSION` in crates/core/src/protocol.rs (checked by a test). */
export const PROTOCOL_VERSION = 1;
