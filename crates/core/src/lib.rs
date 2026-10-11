//! `hollyshow-core`: domain model, command/event contracts and the pure presentation reducer.
//!
//! This crate has **no** Tauri, network, database or async-runtime dependency (only serde and
//! ts-rs) so every rule of the live show is
//! unit-testable. TypeScript bindings are generated from these types with ts-rs
//! (`pnpm gen:types`); never hand-write the TS mirror.

pub mod bible;
pub mod command;
pub mod error;
pub mod event;
pub mod ids;
pub mod playlist;
pub mod protocol;
pub mod reducer;
pub mod song;
pub mod split;
pub mod state;
pub mod text;
pub mod theme;

pub use bible::*;
pub use command::{Command, Ctx, EngineCommand, Restore};
pub use error::{CoreError, WireError};
pub use event::{Event, LibraryScope, Update};
pub use ids::*;
pub use playlist::*;
pub use protocol::{
    negotiate, project, project_view, Ack, ClientMsg, MonitorInfo, QueryRequest, QueryResponse,
    ResyncReason, Role, RoleSnapshot, ServerMsg, CAPABILITIES, PROTOCOL_MIN, PROTOCOL_VERSION,
};
pub use reducer::apply;
pub use song::*;
pub use state::*;
pub use theme::*;
