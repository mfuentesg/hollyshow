//! Bible logic. Owner: builder "bible". Contracts fixed by the architect:
//! * [`ReferenceParser`] resolves free text ("1 Jn 3:16-18", "Juan 3") to a canonical [`Passage`].
//! * [`BibleImporter`] parses a Bible file into [`ImportedBible`] for the storage crate to persist.
//! * Only public-domain texts may be bundled (PLAN §5.2); document sources in `docs/`.

use hollyshow_core::{BibleVersion, Passage, Verse};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BibleError {
    #[error("could not parse reference `{0}`")]
    BadReference(String),
    #[error("malformed Bible file: {0}")]
    Malformed(String),
}

pub trait ReferenceParser {
    /// `language` is a BCP-47 hint (`es`, `en`, `pt`); implementations must accept any language's
    /// book names when unambiguous.
    fn parse(&self, input: &str, language: &str) -> Result<Passage, BibleError>;
}

pub struct ImportedBible {
    pub version: BibleVersion,
    pub verses: Vec<Verse>,
}

pub trait BibleImporter {
    /// `osis`, `usfm`, `zefania`, `opensong`, `csv`, `json`.
    fn format(&self) -> &'static str;
    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError>;
}
