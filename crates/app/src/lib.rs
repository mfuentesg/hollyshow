//! `hollyshow-app`: the use-case layer shared by the Tauri shell and the LAN server, so both
//! behave identically.
//!
//! ```text
//! wire Command --authorise (Role)--> enrich (Bible text, playlist -> show) --> EngineCommand
//!                                                                              --> Engine
//! QueryRequest --authorise--> repositories --> QueryResponse
//! Engine broadcast --> RoleFeed (role projection, lag handling) --> ServerMsg
//! ```
//!
//! Depends on `core`, `engine`, the `storage-api` traits, `songs` and `bible`; knows nothing
//! about Tauri, axum or SQLite.

mod guard;
mod query;
mod service;
mod session;
mod show;

pub use guard::install_panic_hook;
pub use service::{AppService, ClientCtx};
pub use session::RoleFeed;
pub use show::build_show;
