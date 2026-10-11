//! `hollyshow-core`: domain model, command/event contracts and the pure presentation reducer.
//!
//! This crate has **no** Tauri, network or database dependency so every rule of the live show is
//! unit-testable. TypeScript bindings are generated from these types with ts-rs
//! (`pnpm gen:types`); never hand-write the TS mirror.

pub mod bible;
pub mod command;
pub mod engine;
pub mod error;
pub mod event;
pub mod ids;
pub mod playlist;
pub mod protocol;
pub mod reducer;
pub mod song;
pub mod state;
pub mod text;
pub mod theme;

pub use bible::*;
pub use command::Command;
pub use engine::{Engine, Outcome};
pub use error::{CoreError, WireError};
pub use event::{Event, LibraryScope, Update};
pub use ids::*;
pub use playlist::*;
pub use protocol::{
    Ack, ClientMsg, MonitorInfo, QueryRequest, QueryResponse, Role, ServerMsg, PROTOCOL_VERSION,
};
pub use reducer::apply;
pub use song::*;
pub use state::*;
pub use theme::*;
