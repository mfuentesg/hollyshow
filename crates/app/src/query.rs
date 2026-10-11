//! `QueryRequest -> QueryResponse` routing over the repositories.

use hollyshow_core::{QueryRequest, QueryResponse, WireError};
use hollyshow_storage_api::{Repos, StorageError, StorageResult};

pub(crate) fn execute(repos: &Repos, q: &QueryRequest) -> StorageResult<QueryResponse> {
    Ok(match q {
        QueryRequest::SearchSongs { text, limit } => QueryResponse::Songs {
            items: repos.songs.search(text, *limit)?,
        },
        QueryRequest::GetSong { id } => QueryResponse::Song {
            song: repos.songs.get(id)?,
        },
        QueryRequest::ListPlaylists => QueryResponse::Playlists {
            items: repos.playlists.list()?,
        },
        QueryRequest::GetPlaylist { id } => QueryResponse::Playlist {
            playlist: repos.playlists.get(id)?,
        },
        QueryRequest::ListBibleVersions => QueryResponse::BibleVersions {
            items: repos.bibles.list_versions()?,
        },
        QueryRequest::GetPassage { version, passage } => QueryResponse::Passage {
            verses: repos.bibles.passage(version, passage)?,
        },
        QueryRequest::SearchBible {
            text,
            version,
            limit,
        } => QueryResponse::BibleHits {
            items: repos.bibles.search(text, version.as_ref(), *limit)?,
        },
        QueryRequest::ListThemes => QueryResponse::Themes {
            items: repos.themes.list()?,
        },
        QueryRequest::Unknown => {
            return Err(StorageError::Invalid("unsupported query".into()));
        }
    })
}

/// Storage errors become stable wire codes. Backend details (SQL text, file paths) are logged,
/// never sent to a remote.
pub(crate) fn wire_error(e: &StorageError) -> WireError {
    let (code, message) = match e {
        StorageError::NotFound(what) => ("not_found", format!("not found: {what}")),
        StorageError::Conflict(what) => ("conflict", format!("conflict: {what}")),
        StorageError::Invalid(what) => ("invalid", format!("invalid: {what}")),
        StorageError::Backend(detail) => {
            log::error!("storage backend error: {detail}");
            ("storage", "storage error".to_owned())
        }
    };
    WireError {
        code: code.to_owned(),
        message,
    }
}
