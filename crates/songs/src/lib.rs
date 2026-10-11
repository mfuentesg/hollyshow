//! Song logic: expand a song + arrangement into slides, and import songs from other formats.
//!
//! Owner: builder "songs". Contracts fixed by the architect:
//! * [`SlideSplitter`] turns a [`Song`] into [`Slide`]s consumed by `Command::LoadShow`.
//! * [`SongImporter`] implementations parse a file into [`Song`]s (never network, never panic on
//!   malformed input; return [`ImportError`]).

use hollyshow_core::{ArrangementId, Slide, Song, Theme};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported or malformed input: {0}")]
    Malformed(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Splitting parameters. Derived from the theme (`max_lines_per_slide`) unless overridden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitOptions {
    pub max_lines: u8,
    /// Honour manual `---` separators inside section lyrics.
    pub honor_manual_breaks: bool,
}

impl SplitOptions {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            max_lines: theme.max_lines_per_slide.max(1),
            honor_manual_breaks: true,
        }
    }
}

pub trait SlideSplitter {
    fn split(
        &self,
        song: &Song,
        arrangement: Option<&ArrangementId>,
        opts: SplitOptions,
    ) -> Vec<Slide>;
}

pub trait SongImporter {
    /// Short format name: `openlyrics`, `chordpro`, `opensong`, `text`.
    fn format(&self) -> &'static str;
    fn import(&self, input: &str) -> Result<Vec<Song>, ImportError>;
}
