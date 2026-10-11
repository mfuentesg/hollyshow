//! Persistence boundary: only the **traits** in [`repo`] and the error type. No database
//! dependency lives here. The SQLite implementation is `hollyshow-storage-sqlite`;
//! `memory::MemoryStore` (feature `testing`) serves tests and UI development.
//!
//! All methods are synchronous (rusqlite is blocking). Async callers wrap them in
//! `tokio::task::spawn_blocking`.

pub mod error;
#[cfg(any(test, feature = "testing"))]
pub mod memory;
pub mod repo;

pub use error::{StorageError, StorageResult};
pub use repo::{BibleRepo, PlaylistRepo, Repos, SettingsRepo, SongRepo, ThemeRepo};
