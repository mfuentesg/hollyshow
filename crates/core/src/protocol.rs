//! Wire protocol between the LAN server and remotes (PLAN §3.4). JSON text frames over WebSocket.
//!
//! The same [`Command`], [`QueryRequest`] and [`QueryResponse`] shapes are used by the Tauri IPC
//! adapter, so UI code is transport agnostic.

use crate::bible::{BibleHit, BibleVersion, Passage, Verse, VersionCode};
use crate::command::Command;
use crate::error::{CoreError, WireError};
use crate::event::Event;
use crate::ids::{CommandId, PlaylistId, SongId};
use crate::playlist::{Playlist, PlaylistSummary};
use crate::song::{Song, SongSummary};
use crate::state::{PresentationState, StateView};
use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Bumped on any breaking change of the wire format. Servers reject other versions in `hello`.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Role {
    /// Full control.
    Operator,
    /// Read-only plus chords / transposition.
    Musician,
    /// May look up and request Bible passages, nothing else.
    Pastor,
    /// Read-only stage display feed (current, next, timers, messages).
    Stage,
    /// Read-only view of the live slide for phones in the audience.
    Congregation,
}

impl Role {
    /// Authorisation matrix, enforced by the server before a command reaches the engine.
    pub fn can_dispatch(self, cmd: &Command) -> bool {
        match self {
            Role::Operator => true,
            Role::Pastor => matches!(cmd, Command::ShowBible { .. }),
            Role::Musician | Role::Stage | Role::Congregation => false,
        }
    }

    pub fn can_query(self, q: &QueryRequest) -> bool {
        match self {
            Role::Operator => true,
            Role::Musician => matches!(
                q,
                QueryRequest::GetSong { .. } | QueryRequest::GetPlaylist { .. }
            ),
            Role::Pastor => matches!(
                q,
                QueryRequest::ListBibleVersions
                    | QueryRequest::GetPassage { .. }
                    | QueryRequest::SearchBible { .. }
            ),
            Role::Stage | Role::Congregation => false,
        }
    }

    /// Congregation and stage clients get the live view only, never the whole service.
    pub fn sees_full_show(self) -> bool {
        matches!(self, Role::Operator | Role::Musician | Role::Pastor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum QueryRequest {
    SearchSongs {
        text: String,
        limit: u32,
    },
    GetSong {
        id: SongId,
    },
    ListPlaylists,
    GetPlaylist {
        id: PlaylistId,
    },
    ListBibleVersions,
    GetPassage {
        version: VersionCode,
        passage: Passage,
    },
    SearchBible {
        text: String,
        version: Option<VersionCode>,
        limit: u32,
    },
    ListThemes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum QueryResponse {
    Songs { items: Vec<SongSummary> },
    Song { song: Option<Song> },
    Playlists { items: Vec<PlaylistSummary> },
    Playlist { playlist: Option<Playlist> },
    BibleVersions { items: Vec<BibleVersion> },
    Passage { verses: Vec<Verse> },
    BibleHits { items: Vec<BibleHit> },
    Themes { items: Vec<Theme> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
#[allow(clippy::large_enum_variant)]
pub enum ClientMsg {
    /// First frame. `token` resumes a paired device; `pin` pairs a new one.
    Hello {
        protocol: u32,
        role: Role,
        device_name: String,
        token: Option<String>,
        pin: Option<String>,
    },
    Command {
        id: CommandId,
        cmd: Command,
    },
    Query {
        id: CommandId,
        query: QueryRequest,
    },
    /// Ask for a fresh [`ServerMsg::Snapshot`] (after reconnect or `showRevision` change).
    Resync,
    Ping {
        #[ts(type = "number")]
        t: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
#[allow(clippy::large_enum_variant)]
pub enum ServerMsg {
    Welcome {
        protocol: u32,
        role: Role,
        /// Session token to store for reconnects (only when pairing succeeded).
        token: Option<String>,
        server_name: String,
        heartbeat_ms: u32,
    },
    /// Full state, sent after `Welcome` and on `Resync`.
    Snapshot { state: PresentationState },
    /// Cheap state broadcast after every change; `showRevision` tells when to resync.
    State { view: StateView },
    Ack {
        id: CommandId,
        ok: bool,
        error: Option<WireError>,
        #[ts(type = "number")]
        version: u64,
    },
    QueryResult {
        id: CommandId,
        result: Option<QueryResponse>,
        error: Option<WireError>,
    },
    Event {
        #[ts(type = "number")]
        version: u64,
        event: Event,
    },
    Pong {
        #[ts(type = "number")]
        t: i64,
    },
    /// Fatal: the server closes the socket after sending this (bad protocol, bad PIN, kicked...).
    Reject { error: WireError },
}

/// A physical display as reported by the desktop shell, for the "choose output screen" dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MonitorInfo {
    pub index: u32,
    pub name: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

/// Result of dispatching a command, identical for Tauri IPC and WebSocket clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Ack {
    pub ok: bool,
    /// State version after the command (or the current one when rejected).
    #[ts(type = "number")]
    pub version: u64,
    pub error: Option<WireError>,
    /// The command id was already processed; nothing was re-applied.
    pub duplicate: bool,
}

impl ServerMsg {
    pub fn ack_ok(id: CommandId, version: u64) -> Self {
        Self::Ack {
            id,
            ok: true,
            error: None,
            version,
        }
    }

    pub fn ack_err(id: CommandId, version: u64, err: &CoreError) -> Self {
        Self::Ack {
            id,
            ok: false,
            error: Some(err.into()),
            version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_wire_shape_matches_plan() {
        let c: ClientMsg = serde_json::from_value(json!({
            "type": "command", "id": "c-123", "cmd": { "kind": "next", "expectedVersion": 418 }
        }))
        .unwrap();
        match c {
            ClientMsg::Command {
                id,
                cmd: Command::Next { expected_version },
            } => {
                assert_eq!(id.as_str(), "c-123");
                assert_eq!(expected_version, Some(418));
            }
            other => panic!("unexpected {other:?}"),
        }
        let blackout: ClientMsg = serde_json::from_value(
            json!({"type":"command","id":"c-125","cmd":{"kind":"blackout","on":true}}),
        )
        .unwrap();
        assert!(matches!(
            blackout,
            ClientMsg::Command {
                cmd: Command::Blackout { on: true },
                ..
            }
        ));
    }

    #[test]
    fn hello_and_ack_serialise_camel_case() {
        let hello = ClientMsg::Hello {
            protocol: PROTOCOL_VERSION,
            role: Role::Pastor,
            device_name: "Pixel".into(),
            token: None,
            pin: Some("123456".into()),
        };
        let v = serde_json::to_value(&hello).unwrap();
        assert_eq!(v["type"], "hello");
        assert_eq!(v["deviceName"], "Pixel");
        let ack = serde_json::to_value(ServerMsg::ack_ok("c-1".into(), 7)).unwrap();
        assert_eq!(
            ack,
            json!({"type":"ack","id":"c-1","ok":true,"error":null,"version":7})
        );
    }

    #[test]
    fn pastor_may_only_show_bible() {
        assert!(!Role::Pastor.can_dispatch(&Command::Blackout { on: true }));
        assert!(!Role::Congregation.can_dispatch(&Command::Next {
            expected_version: None
        }));
        assert!(Role::Operator.can_dispatch(&Command::Blackout { on: true }));
        assert!(Role::Pastor.can_query(&QueryRequest::ListBibleVersions));
        assert!(!Role::Pastor.can_query(&QueryRequest::ListPlaylists));
    }
}
