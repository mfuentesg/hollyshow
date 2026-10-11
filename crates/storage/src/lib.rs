//! Persistence boundary. `server` and `desktop` depend on the **traits** in [`repo`]; the SQLite
//! implementation (rusqlite + FTS5, migrations, backups) is added in this crate behind them, and
//! [`memory::MemoryStore`] serves tests and UI development.
//!
//! All methods are synchronous (rusqlite is blocking). Async callers wrap them in
//! `tokio::task::spawn_blocking`.

pub mod error;
pub mod host;
pub mod memory;
pub mod repo;

pub use error::{StorageError, StorageResult};
pub use repo::{BibleRepo, PlaylistRepo, Repos, SettingsRepo, SongRepo, ThemeRepo};
