//! Error types of the core crate.

use crate::ids::ItemId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Why the reducer rejected a command. Rejections never mutate state.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    #[error("version mismatch: command expected {expected}, state is at {actual}")]
    VersionMismatch { expected: u64, actual: u64 },
    #[error("unknown item {0}")]
    UnknownItem(ItemId),
    #[error("slide {slide_index} out of range for item {item_id} ({slide_count} slides)")]
    SlideOutOfRange {
        item_id: ItemId,
        slide_index: u32,
        slide_count: u32,
    },
    #[error("nothing in preview to take live")]
    NothingInPreview,
    #[error("Bible passage was not resolved by the host before reaching the reducer")]
    BibleNotResolved,
    #[error("unknown timer {0}")]
    UnknownTimer(String),
    #[error("command not permitted for this role")]
    Forbidden,
    #[error("invalid command: {0}")]
    Invalid(String),
}

impl CoreError {
    /// Stable machine readable code, safe to match on in clients.
    pub fn code(&self) -> &'static str {
        match self {
            Self::VersionMismatch { .. } => "version_mismatch",
            Self::UnknownItem(_) => "unknown_item",
            Self::SlideOutOfRange { .. } => "slide_out_of_range",
            Self::NothingInPreview => "nothing_in_preview",
            Self::BibleNotResolved => "bible_not_resolved",
            Self::UnknownTimer(_) => "unknown_timer",
            Self::Forbidden => "forbidden",
            Self::Invalid(_) => "invalid",
        }
    }
}

/// Error as seen on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WireError {
    pub code: String,
    pub message: String,
}

impl From<&CoreError> for WireError {
    fn from(e: &CoreError) -> Self {
        Self {
            code: e.code().to_owned(),
            message: e.to_string(),
        }
    }
}

impl From<CoreError> for WireError {
    fn from(e: CoreError) -> Self {
        Self::from(&e)
    }
}
