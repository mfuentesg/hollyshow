//! Wire protocol between the LAN server and remotes (PLAN §3.4). JSON text frames over WebSocket.
//!
//! The same [`Command`], [`QueryRequest`] and [`QueryResponse`] shapes are used by the Tauri IPC
//! adapter, so UI code is transport agnostic.

use crate::bible::{BibleHit, BibleVersion, Passage, Verse, VersionCode};
use crate::command::Command;
use crate::error::{CoreError, WireError};
use crate::event::Event;
use crate::ids::{CommandId, PlaylistId, QueryId, SongId};
use crate::playlist::{Playlist, PlaylistSummary};
use crate::song::{Song, SongSummary};
use crate::state::{PresentationState, ShowEntry, StateView};
use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Newest protocol revision this build speaks. Bumped on any breaking change of the wire format.
pub const PROTOCOL_VERSION: u32 = 1;
/// Oldest revision this build still speaks. `hello` carries the client's `[min, max]` range and
/// the server answers with the highest revision both support (see [`negotiate`]).
pub const PROTOCOL_MIN: u32 = 1;
/// Optional features advertised in `hello`/`welcome` so peers can degrade instead of failing.
pub const CAPABILITIES: &[&str] = &["bible-parallel", "timers", "stage-feed", "live-edit"];

/// Picks the protocol revision for a client that supports `[client_min, client_max]`.
pub fn negotiate(client_min: u32, client_max: u32) -> Result<u32, WireError> {
    let v = client_max.min(PROTOCOL_VERSION);
    if client_min > client_max || v < client_min.max(PROTOCOL_MIN) {
        return Err(WireError {
            code: "protocol_mismatch".into(),
            message: format!(
                "client speaks {client_min}..={client_max}, server speaks {PROTOCOL_MIN}..={PROTOCOL_VERSION}"
            ),
        });
    }
    Ok(v)
}

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
            _ if matches!(cmd, Command::Unknown) => false,
            Role::Operator => true,
            Role::Pastor => matches!(cmd, Command::ShowBible { .. }),
            Role::Musician | Role::Stage | Role::Congregation => false,
        }
    }

    pub fn can_query(self, q: &QueryRequest) -> bool {
        match self {
            _ if matches!(q, QueryRequest::Unknown) => false,
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

    /// Whether the role receives the expanded service (every slide of every item).
    pub fn sees_full_show(self) -> bool {
        matches!(self, Role::Operator | Role::Musician | Role::Pastor)
    }

    /// Whether the role sees the queued next slide. Stage displays need it; the audience does not.
    pub fn sees_preview(self) -> bool {
        !matches!(self, Role::Congregation)
    }

    /// Whether the role sees timers, stage messages, output looks and service mode.
    pub fn sees_operations(self) -> bool {
        !matches!(self, Role::Congregation)
    }
}

/// The only way state reaches a network client: everything a role may not see is removed here,
/// by construction, instead of relying on each sender to remember a flag.
pub fn project_view(view: &StateView, role: Role) -> StateView {
    let mut v = view.clone();
    if !role.sees_preview() {
        v.preview = None;
    }
    if !role.sees_operations() {
        v.stage_message = None;
        v.timers.clear();
        v.outputs.clear();
        v.output_layers.clear();
        v.item_clock = None;
        v.service_mode = false;
    }
    if !role.sees_full_show() {
        // These roles never hold a show, so a revision bump must not trigger pointless resyncs.
        v.show_revision = 0;
    }
    v
}

/// Role-filtered snapshot: `show` is `None` for roles that must not see the whole service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleSnapshot {
    pub view: StateView,
    pub show: Option<Vec<ShowEntry>>,
}

pub fn project(state: &PresentationState, role: Role) -> RoleSnapshot {
    RoleSnapshot {
        view: project_view(&state.view(), role),
        show: role.sees_full_show().then(|| state.show.clone()),
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
    /// A query from a newer protocol revision; answered with `unsupported`.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum QueryResponse {
    Songs {
        items: Vec<SongSummary>,
    },
    Song {
        song: Option<Song>,
    },
    Playlists {
        items: Vec<PlaylistSummary>,
    },
    Playlist {
        playlist: Option<Playlist>,
    },
    BibleVersions {
        items: Vec<BibleVersion>,
    },
    Passage {
        verses: Vec<Verse>,
    },
    BibleHits {
        items: Vec<BibleHit>,
    },
    Themes {
        items: Vec<Theme>,
    },
    /// Placeholder so old clients survive new response kinds.
    #[serde(other)]
    Unknown,
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
        /// Lowest and highest protocol revision the client speaks.
        protocol_min: u32,
        protocol_max: u32,
        role: Role,
        device_name: String,
        token: Option<String>,
        pin: Option<String>,
        /// Optional features the client understands (see [`CAPABILITIES`]).
        #[serde(default)]
        capabilities: Vec<String>,
    },
    Command {
        id: CommandId,
        cmd: Command,
    },
    Query {
        id: QueryId,
        query: QueryRequest,
    },
    /// Ask for a fresh [`ServerMsg::Snapshot`] (after reconnect or `showRevision` change).
    Resync,
    Ping {
        #[ts(type = "number")]
        t: i64,
    },
    /// A frame type from a newer protocol revision; answered with an `unsupported` error.
    #[serde(other)]
    Unknown,
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
        /// Negotiated protocol revision.
        protocol: u32,
        role: Role,
        /// Session token to store for reconnects (only when pairing succeeded).
        token: Option<String>,
        server_name: String,
        heartbeat_ms: u32,
        capabilities: Vec<String>,
        /// Boot epoch of the host (see `StateView::epoch`). Relative commands must echo it.
        #[ts(type = "number")]
        epoch: u64,
        /// Server wall clock (epoch ms) at send time; clients derive `offset = now_ms - local`
        /// to render timers that agree with the projector.
        #[ts(type = "number")]
        now_ms: i64,
    },
    /// Role-filtered full state, sent after `Welcome` and on `Resync`.
    Snapshot {
        snapshot: RoleSnapshot,
        #[ts(type = "number")]
        now_ms: i64,
    },
    /// Cheap role-filtered state broadcast after every change; `showRevision` tells when to
    /// resync.
    State {
        view: StateView,
        #[ts(type = "number")]
        now_ms: i64,
    },
    /// Same verdict as the IPC [`Ack`], tagged with the command id it answers.
    Ack { id: CommandId, ack: Ack },
    QueryResult {
        id: QueryId,
        result: Option<QueryResponse>,
        error: Option<WireError>,
    },
    /// `version` is the state version after the change; consecutive events from one client must
    /// have consecutive versions, otherwise it missed some and sends `Resync`.
    Event {
        #[ts(type = "number")]
        version: u64,
        event: Event,
    },
    /// The server could not deliver every update (slow client); a fresh `State` precedes this and
    /// the client should request a `Snapshot`.
    Resync { reason: ResyncReason },
    Pong {
        #[ts(type = "number")]
        t: i64,
    },
    /// Fatal: the server closes the socket after sending this (bad protocol, bad PIN, kicked...).
    Reject { error: WireError },
    /// A message type from a newer protocol revision; ignored by old clients.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ResyncReason {
    /// The client's receive queue overflowed and updates were dropped.
    Lagged,
    /// A relative command carried the epoch of a previous host run; the client's picture of the
    /// state is from before a restart and must be replaced by a fresh `Snapshot`.
    EpochMismatch,
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
    pub fn ack(id: CommandId, ack: Ack) -> Self {
        Self::Ack { id, ack }
    }

    /// The messages to send one client for an engine update: the role-filtered state followed by
    /// each visible event. This is the only way an [`Update`] reaches the network.
    pub fn from_update(update: &crate::event::Update, role: Role, now_ms: i64) -> Vec<ServerMsg> {
        let u = update.for_role(role);
        let mut out = Vec::with_capacity(1 + u.events.len());
        out.push(ServerMsg::State {
            view: u.view,
            now_ms,
        });
        out.extend(u.events.into_iter().map(|event| ServerMsg::Event {
            version: u.version,
            event,
        }));
        out
    }
}

impl Ack {
    pub fn ok(version: u64, duplicate: bool) -> Self {
        Self {
            ok: true,
            version,
            error: None,
            duplicate,
        }
    }

    pub fn err(version: u64, err: &CoreError, duplicate: bool) -> Self {
        Self {
            ok: false,
            version,
            error: Some(err.into()),
            duplicate,
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
            "type": "command", "id": "c-123", "cmd": { "kind": "next", "expectedVersion": 418, "epoch": 77 }
        }))
        .unwrap();
        match c {
            ClientMsg::Command {
                id,
                cmd:
                    Command::Next {
                        expected_version,
                        epoch,
                    },
            } => {
                assert_eq!(id.as_str(), "c-123");
                assert_eq!(expected_version, Some(418));
                assert_eq!(epoch, Some(77));
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
            protocol_min: PROTOCOL_MIN,
            protocol_max: PROTOCOL_VERSION,
            role: Role::Pastor,
            device_name: "Pixel".into(),
            token: None,
            pin: Some("123456".into()),
            capabilities: vec!["timers".into()],
        };
        let v = serde_json::to_value(&hello).unwrap();
        assert_eq!(v["type"], "hello");
        assert_eq!(v["deviceName"], "Pixel");
        assert_eq!(v["protocolMax"], PROTOCOL_VERSION);
        let ack = serde_json::to_value(ServerMsg::ack("c-1".into(), Ack::ok(7, false))).unwrap();
        assert_eq!(
            ack,
            json!({"type":"ack","id":"c-1","ack":{"ok":true,"version":7,"error":null,"duplicate":false}})
        );
    }

    #[test]
    fn unknown_kinds_degrade_instead_of_failing_the_frame() {
        let c: ClientMsg = serde_json::from_value(json!({"type":"teleport","x":1})).unwrap();
        assert_eq!(c, ClientMsg::Unknown);
        let cmd: Command = serde_json::from_value(json!({"kind":"freeze","on":true})).unwrap();
        assert_eq!(cmd, Command::Unknown);
        let ev: Event = serde_json::from_value(json!({"kind":"somethingNew"})).unwrap();
        assert_eq!(ev, Event::Unknown);
        let m: ServerMsg = serde_json::from_value(json!({"type":"newThing"})).unwrap();
        assert_eq!(m, ServerMsg::Unknown);
        let q: QueryRequest = serde_json::from_value(json!({"kind":"newQuery"})).unwrap();
        assert!(!Role::Operator.can_query(&q));
        assert!(!Role::Operator.can_dispatch(&Command::Unknown));
    }

    #[test]
    fn wire_show_bible_cannot_carry_a_slide() {
        // A hostile `resolved` field is not part of the type, so it is dropped on parse.
        let cmd: Command = serde_json::from_value(json!({
            "kind":"showBible",
            "passage":{"start":{"book":"John","chapter":3,"verse":16},"end":{"book":"John","chapter":3,"verse":16}},
            "versions":["RVR1909"], "layout":"columns", "target":"live",
            "resolved": {"id":"evil","kind":"text","lines":["pwned"]}
        }))
        .unwrap();
        let back = serde_json::to_value(&cmd).unwrap();
        assert!(back.get("resolved").is_none());
    }

    #[test]
    fn protocol_negotiation_picks_highest_common_revision() {
        assert_eq!(negotiate(1, 1).unwrap(), 1);
        assert_eq!(negotiate(1, 9).unwrap(), PROTOCOL_VERSION);
        assert_eq!(negotiate(9, 12).unwrap_err().code, "protocol_mismatch");
        assert_eq!(negotiate(3, 2).unwrap_err().code, "protocol_mismatch");
    }

    fn sample_state() -> PresentationState {
        use crate::state::{Cue, Slide, StageMessage};
        let cue = |id: &str| Cue {
            item_id: None,
            slide_index: 0,
            slide: Slide::blank(id),
        };
        PresentationState {
            show: vec![ShowEntry {
                item_id: "a".into(),
                title: "a".into(),
                slides: vec![Slide::blank("a:0")],
                duration_secs: None,
            }],
            live: Some(cue("live")),
            preview: Some(cue("next")),
            stage_message: Some(StageMessage {
                text: "hola".into(),
            }),
            show_revision: 4,
            ..Default::default()
        }
    }

    #[test]
    fn projection_hides_what_a_role_must_not_see() {
        let st = sample_state();
        let op = project(&st, Role::Operator);
        assert!(op.show.is_some() && op.view.preview.is_some());
        let stage = project(&st, Role::Stage);
        assert!(stage.show.is_none());
        assert!(stage.view.preview.is_some() && stage.view.stage_message.is_some());
        let crowd = project(&st, Role::Congregation);
        assert!(crowd.show.is_none());
        assert!(crowd.view.preview.is_none());
        assert!(crowd.view.stage_message.is_none() && crowd.view.timers.is_empty());
        assert!(crowd.view.live.is_some());
        assert_eq!(crowd.view.show_revision, 0);
    }

    #[test]
    fn congregation_never_sees_layers_or_item_clock() {
        let mut st = sample_state();
        st.item_clock = Some(crate::state::ItemClock {
            item_id: "a".into(),
            planned_secs: 60,
            started_at_ms: 1,
        });
        let crowd = project(&st, Role::Congregation).view;
        assert!(crowd.output_layers.is_empty() && crowd.item_clock.is_none());
        let stage = project(&st, Role::Stage).view;
        assert!(stage.item_clock.is_some() && !stage.output_layers.is_empty());
    }

    #[test]
    fn new_commands_have_stable_wire_shapes() {
        let c: Command = serde_json::from_value(json!({
            "kind": "setOutputLayers",
            "output": "lobby",
            "layers": {"background": null, "media": null, "text": true,
                       "overlay": "Hola", "blackout": false}
        }))
        .unwrap();
        assert!(matches!(c, Command::SetOutputLayers { .. }));
        let c: Command = serde_json::from_value(json!({"kind": "resetItemClock"})).unwrap();
        assert_eq!(c, Command::ResetItemClock);
        let c: Command =
            serde_json::from_value(json!({"kind": "removeOutput", "output": "lobby"})).unwrap();
        assert!(matches!(c, Command::RemoveOutput { .. }));
        assert!(Role::Operator.can_dispatch(&Command::ResetItemClock));
        assert!(!Role::Pastor.can_dispatch(&Command::ResetItemClock));
    }

    #[test]
    fn slides_without_size_group_still_deserialize() {
        let s: crate::state::Slide = serde_json::from_value(json!({
            "id": "x", "kind": "lyrics", "label": null, "lines": ["a"],
            "media": null, "footer": null, "themeId": null
        }))
        .unwrap();
        assert!(s.size_group.is_none());
    }

    #[test]
    fn updates_are_filtered_per_role() {
        use crate::event::Update;
        let st = sample_state();
        let up = Update {
            version: 5,
            view: st.view(),
            events: vec![
                Event::BlackoutChanged { on: true },
                Event::LibraryChanged {
                    scope: crate::event::LibraryScope::Songs,
                },
                Event::StageMessageChanged {
                    text: Some("x".into()),
                },
            ],
        };
        let msgs = ServerMsg::from_update(&up, Role::Congregation, 9);
        assert_eq!(msgs.len(), 2);
        match &msgs[0] {
            ServerMsg::State { view, now_ms } => {
                assert!(view.preview.is_none());
                assert_eq!(*now_ms, 9);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(ServerMsg::from_update(&up, Role::Operator, 0).len(), 4);
    }

    #[test]
    fn pastor_may_only_show_bible() {
        assert!(!Role::Pastor.can_dispatch(&Command::Blackout { on: true }));
        assert!(!Role::Congregation.can_dispatch(&Command::Next {
            expected_version: None,
            epoch: None
        }));
        assert!(Role::Operator.can_dispatch(&Command::Blackout { on: true }));
        assert!(Role::Pastor.can_query(&QueryRequest::ListBibleVersions));
        assert!(!Role::Pastor.can_query(&QueryRequest::ListPlaylists));
    }
}
