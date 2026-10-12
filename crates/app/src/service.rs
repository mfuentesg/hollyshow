//! [`AppService`]: the single entry point for commands and queries from every transport.

use crate::guard::contain;
use crate::query;
use crate::session::RoleFeed;
use crate::show::build_show;
use crate::usage::{local_day, now_ms, UsageDedupe, UsageLogger, REPEAT_WINDOW_MS};
use crate::writer::SessionWriter;
mod library;
use hollyshow_bible::{build_bible_slide, VersionText};
use hollyshow_core::{
    project, Ack, BibleVersion, Command, CommandId, CoreError, Cue, EngineCommand, Event,
    ItemContent, ItemId, LibraryScope, ParallelLayout, Passage, PlaylistId, QueryRequest,
    QueryResponse, Restore, Role, ServerMsg, SessionId, ShowEntry, Slide, SongId, Target, Timer,
    Verse, VersionCode, WireError,
};
use hollyshow_engine::{Dispatched, Engine, Outcome};
use hollyshow_songs::SlideSplitter;
use hollyshow_storage_api::Repos;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

/// How long an explicit flush (exit, panic hook) waits for the writer thread.
const FLUSH_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);
/// Setting key holding the persisted session (see [`SavedSession`]).
const SESSION_KEY: &str = "session.v1";
/// A parallel Bible slide with more columns than this is unreadable and a cheap way to spam.
const MAX_BIBLE_VERSIONS: usize = 6;
/// A single Bible slide never needs more verses than this.
const MAX_PASSAGE_VERSES: usize = 60;

/// Who is asking. Built by the transport: the Tauri shell uses [`ClientCtx::local_operator`], the
/// WebSocket server creates one per authenticated connection.
#[derive(Debug, Clone)]
pub struct ClientCtx {
    pub session: SessionId,
    pub role: Role,
}

impl ClientCtx {
    pub fn new(session: SessionId, role: Role) -> Self {
        Self { session, role }
    }

    /// True for the in-process operator window; every other session is a LAN client.
    pub fn is_local(&self) -> bool {
        self.session.as_str() == "local"
    }

    /// The operator window of this machine.
    pub fn local_operator() -> Self {
        Self::new(SessionId::new("local"), Role::Operator)
    }
}

/// What survives a restart: which service is open, what is on screen and the timers.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct SavedSession {
    #[serde(default)]
    active_playlist: Option<PlaylistId>,
    #[serde(default)]
    live: Option<Cue>,
    #[serde(default)]
    timers: Vec<Timer>,
    #[serde(default)]
    service_mode: bool,
}

#[derive(Default)]
struct HostState {
    active_playlist: Option<PlaylistId>,
    /// Live edits ("reflow"), keyed by item and slide index. They overlay the show for the current
    /// service only: they are re-applied after every rebuild and cleared when another playlist is
    /// opened. They never write back to the song library.
    live_edits: HashMap<(ItemId, u32), Slide>,
    last_saved: Option<String>,
    /// Song behind each song item of the open service, so going live resolves the song in memory.
    song_items: HashMap<ItemId, SongId>,
    /// CCLI dedupe state (see `usage::UsageDedupe`).
    usage_dedupe: UsageDedupe,
}

pub struct AppService {
    /// Serialises library writes with the `ServiceMode` command (see `library`).
    write_gate: Mutex<()>,
    engine: Arc<Engine>,
    repos: Repos,
    splitter: Arc<dyn SlideSplitter>,
    host: Mutex<HostState>,
    writer: SessionWriter,
    usage: UsageLogger,
}

fn host_session() -> SessionId {
    SessionId::new("host")
}

fn storage_to_core(e: &hollyshow_storage_api::StorageError) -> CoreError {
    CoreError::Invalid(query::wire_error(e).message)
}

impl AppService {
    pub fn new(repos: Repos, engine: Arc<Engine>, splitter: Arc<dyn SlideSplitter>) -> Self {
        let writer = SessionWriter::spawn(repos.settings.clone(), SESSION_KEY);
        let usage = UsageLogger::spawn(repos.clone());
        let mut host = HostState::default();
        let now = now_ms();
        // Survive restarts: what was logged in the last window still counts as already used,
        // and only if it happened today (a new service day starts clean).
        match repos.songs.recent_usage(now - REPEAT_WINDOW_MS) {
            Ok(rows) => {
                let today = local_day(now);
                host.usage_dedupe
                    .seed(rows.into_iter().filter(|(_, at)| local_day(*at) == today));
            }
            Err(e) => log::warn!("usage history: {e}"),
        }
        Self {
            write_gate: Mutex::new(()),
            engine,
            repos,
            splitter,
            host: Mutex::new(host),
            writer,
            usage,
        }
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub fn repos(&self) -> &Repos {
        &self.repos
    }

    fn host(&self) -> MutexGuard<'_, HostState> {
        self.host.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ---- commands -------------------------------------------------------------------------

    /// Authorises, enriches and applies a wire command. Never panics out: an internal failure is
    /// answered with an `internal` error and the engine keeps running.
    pub fn dispatch(&self, ctx: &ClientCtx, id: Option<&CommandId>, cmd: Command) -> Ack {
        let acked = contain("dispatch", || self.dispatch_inner(ctx, id, cmd));
        acked.unwrap_or_else(|| self.ack_err(&CoreError::Internal, false))
    }

    fn dispatch_inner(&self, ctx: &ClientCtx, id: Option<&CommandId>, cmd: Command) -> Ack {
        if matches!(cmd, Command::Unknown) {
            return self.ack_err(&CoreError::Unsupported("unknown command".into()), false);
        }
        if !ctx.role.can_dispatch(&cmd) {
            return self.ack_err(&CoreError::Forbidden, false);
        }
        // Relative commands are not idempotent: a remote client must say which state it saw, or a
        // retry / double tap would skip slides.
        if !ctx.is_local()
            && matches!(
                cmd,
                Command::Next {
                    expected_version: None,
                    ..
                } | Command::Next { epoch: None, .. }
                    | Command::Prev {
                        expected_version: None,
                        ..
                    }
                    | Command::Prev { epoch: None, .. }
            )
        {
            return self.ack_err(
                &CoreError::Invalid(
                    "relative commands from remote clients need expectedVersion and epoch".into(),
                ),
                false,
            );
        }
        let session = &ctx.session;
        let ack = match cmd {
            Command::ShowBible {
                passage,
                versions,
                layout,
                target,
            } => match self.resolve_bible(&passage, &versions, layout) {
                Ok(slide) => self.ack_from(self.engine.dispatch(
                    session,
                    id,
                    EngineCommand::ShowBible {
                        // Only the operator may take a passage live; others just stage it.
                        target: if ctx.role == Role::Operator {
                            target
                        } else {
                            Target::Preview
                        },
                        layout,
                        slide,
                    },
                )),
                Err(e) => self.ack_err(&e, false),
            },
            // Swapping or rebuilding the show mid-service is as destructive as editing it.
            Command::SetActivePlaylist { .. } | Command::RefreshShow if self.is_locked() => {
                self.ack_err(&CoreError::Locked, false)
            }
            Command::SetActivePlaylist { id: playlist } => {
                self.ack_plain(self.set_active_playlist(&playlist))
            }
            Command::RefreshShow => self.ack_plain(self.refresh_show()),
            Command::ServiceMode { on } => {
                // Taking the gate means no library write is half-way when the lock engages.
                let _gate = self.gate();
                self.ack_from(
                    self.engine
                        .dispatch(session, id, Command::ServiceMode { on }.into()),
                )
            }
            Command::UpdateSlide {
                item_id,
                slide_index,
                slide,
            } => self.live_edit(session, id, item_id, slide_index, slide),
            other => self.ack_from(self.engine.dispatch(session, id, other.into())),
        };
        if ack.ok && !ack.duplicate {
            self.note_usage();
            self.persist();
        }
        ack
    }

    /// Logs the live song for the CCLI report, whatever way it went live (service item, command
    /// palette, remote). Cheap and non-blocking: only the insert happens on the usage thread.
    fn note_usage(&self) {
        let item = self.engine.view().live.and_then(|c| c.item_id);
        let mut host = self.host();
        let song = item.and_then(|i| host.song_items.get(&i).cloned());
        let now = now_ms();
        if host
            .usage_dedupe
            .observe(song.as_ref(), now, local_day(now))
        {
            drop(host);
            if let Some(song) = song {
                self.usage.submit(song, now);
            }
        }
    }

    fn ack_from(&self, d: Dispatched) -> Ack {
        match d.result {
            Ok(o) => Ack::ok(o.version, d.duplicate),
            Err(e) => self.ack_err(&e, d.duplicate),
        }
    }

    fn ack_plain(&self, r: Result<Outcome, CoreError>) -> Ack {
        match r {
            Ok(o) => Ack::ok(o.version, false),
            Err(e) => self.ack_err(&e, false),
        }
    }

    fn ack_err(&self, e: &CoreError, duplicate: bool) -> Ack {
        Ack::err(self.engine.view().version, e, duplicate)
    }

    /// Replaces a slide's text. The slide id is pinned to the existing one so the edit stays
    /// attached to its slot when the show is rebuilt.
    fn live_edit(
        &self,
        session: &SessionId,
        id: Option<&CommandId>,
        item_id: ItemId,
        slide_index: u32,
        mut slide: Slide,
    ) -> Ack {
        let existing = self.engine.with_state(|s| {
            s.entry(&item_id)
                .and_then(|e| e.slides.get(slide_index as usize))
                .map(|x| x.id.clone())
        });
        if let Some(existing) = existing {
            slide.id = existing;
        }
        let cmd = Command::UpdateSlide {
            item_id: item_id.clone(),
            slide_index,
            slide: slide.clone(),
        };
        let d = self.engine.dispatch(session, id, cmd.into());
        if d.result.is_ok() {
            self.host().live_edits.insert((item_id, slide_index), slide);
        }
        self.ack_from(d)
    }

    /// Host-internal: replaces the show with `entries` (demo data, imports). Not reachable from
    /// the wire.
    pub fn load_entries(&self, entries: Vec<ShowEntry>) -> Result<Outcome, CoreError> {
        self.engine
            .dispatch(&host_session(), None, EngineCommand::LoadShow { entries })
            .result
    }

    // ---- playlists and the show ---------------------------------------------------------

    /// Opens `playlist` as the active service and rebuilds the show from it.
    pub fn set_active_playlist(&self, playlist: &PlaylistId) -> Result<Outcome, CoreError> {
        let mut host = self.host();
        let pl = self
            .repos
            .playlists
            .get(playlist)
            .map_err(|e| storage_to_core(&e))?
            .ok_or_else(|| CoreError::Invalid(format!("unknown playlist {playlist}")))?;
        if host.active_playlist.as_ref() != Some(playlist) {
            host.live_edits.clear();
        }
        host.active_playlist = Some(playlist.clone());
        let out = self.load_playlist(&mut host, &pl);
        self.persist_locked(&mut host);
        out
    }

    /// Rebuilds the show from the active playlist, keeping live edits on top.
    pub fn refresh_show(&self) -> Result<Outcome, CoreError> {
        let mut host = self.host();
        let id = host
            .active_playlist
            .clone()
            .ok_or_else(|| CoreError::Invalid("no active playlist".into()))?;
        let pl = self
            .repos
            .playlists
            .get(&id)
            .map_err(|e| storage_to_core(&e))?
            .ok_or_else(|| CoreError::Invalid(format!("active playlist {id} was deleted")))?;
        self.load_playlist(&mut host, &pl)
    }

    fn load_playlist(
        &self,
        host: &mut HostState,
        pl: &hollyshow_core::Playlist,
    ) -> Result<Outcome, CoreError> {
        host.song_items = pl
            .items
            .iter()
            .filter_map(|i| match &i.content {
                ItemContent::Song { song_id, .. } => Some((i.id.clone(), song_id.clone())),
                _ => None,
            })
            .collect();
        let mut entries = build_show(&self.repos, self.splitter.as_ref(), pl);
        for ((item, index), edit) in &host.live_edits {
            let slot = entries
                .iter_mut()
                .find(|e| &e.item_id == item)
                .and_then(|e| e.slides.get_mut(*index as usize));
            if let Some(slot) = slot.filter(|s| s.id == edit.id) {
                *slot = edit.clone();
            }
        }
        self.engine
            .dispatch(&host_session(), None, EngineCommand::LoadShow { entries })
            .result
    }

    /// Tell the app a library area changed (an importer, a song editor...). Publishes the event
    /// for clients and rebuilds the show when content the show is made of changed.
    pub fn library_changed(&self, scope: LibraryScope) {
        if matches!(
            scope,
            LibraryScope::Songs | LibraryScope::Bible | LibraryScope::Themes
        ) {
            self.refresh_active();
        }
        self.engine
            .publish_host_event(Event::LibraryChanged { scope });
    }

    /// Tell the app a playlist was saved; rebuilds the show when it is the active one.
    pub fn playlist_saved(&self, id: &PlaylistId) {
        if self.host().active_playlist.as_ref() == Some(id) {
            self.refresh_active();
        }
        self.engine
            .publish_host_event(Event::PlaylistChanged { id: id.clone() });
    }

    fn refresh_active(&self) {
        let has_active = self.host().active_playlist.is_some();
        if has_active {
            if let Err(e) = self.refresh_show() {
                log::warn!("could not refresh the show: {e}");
            }
        }
    }

    // ---- Bible ------------------------------------------------------------------------------

    fn resolve_bible(
        &self,
        passage: &Passage,
        versions: &[VersionCode],
        layout: ParallelLayout,
    ) -> Result<Slide, CoreError> {
        if versions.is_empty() {
            return Err(CoreError::Invalid(
                "ShowBible needs at least one version".into(),
            ));
        }
        if versions.len() > MAX_BIBLE_VERSIONS {
            return Err(CoreError::Invalid(format!(
                "at most {MAX_BIBLE_VERSIONS} versions at once"
            )));
        }
        let mut loaded: Vec<(BibleVersion, Vec<Verse>)> = Vec::with_capacity(versions.len());
        for code in versions {
            let meta = self
                .repos
                .bibles
                .get_version(code)
                .map_err(|e| storage_to_core(&e))?
                .ok_or_else(|| CoreError::Invalid(format!("unknown Bible version {}", code.0)))?;
            let verses = self
                .repos
                .bibles
                .passage(code, passage)
                .map_err(|e| storage_to_core(&e))?;
            if verses.len() > MAX_PASSAGE_VERSES {
                return Err(CoreError::Invalid(format!(
                    "passage longer than {MAX_PASSAGE_VERSES} verses"
                )));
            }
            loaded.push((meta, verses));
        }
        let cols: Vec<VersionText<'_>> = loaded
            .iter()
            .map(|(version, verses)| VersionText { version, verses })
            .collect();
        Ok(build_bible_slide(passage, layout, &cols))
    }

    // ---- queries ----------------------------------------------------------------------------

    pub fn query(&self, ctx: &ClientCtx, q: &QueryRequest) -> Result<QueryResponse, WireError> {
        if matches!(q, QueryRequest::Unknown) {
            return Err(WireError::from(CoreError::Unsupported(
                "unknown query".into(),
            )));
        }
        if !ctx.role.can_query(q) {
            return Err(WireError::from(CoreError::Forbidden));
        }
        match contain("query", || query::execute(&self.repos, q)) {
            Some(Ok(r)) => Ok(r),
            Some(Err(e)) => Err(query::wire_error(&e)),
            None => Err(WireError::from(CoreError::Internal)),
        }
    }

    // ---- state for clients ------------------------------------------------------------------

    /// Role-filtered full state for a client that just connected or asked to resync.
    pub fn snapshot_msg(&self, role: Role) -> ServerMsg {
        ServerMsg::Snapshot {
            snapshot: self.engine.with_state(|s| project(s, role)),
            now_ms: self.engine.now_ms(),
        }
    }

    /// Subscribe a client to role-filtered updates. Call before [`Self::snapshot_msg`].
    pub fn feed(&self, role: Role) -> RoleFeed {
        RoleFeed::new(self.engine.clone(), role)
    }

    // ---- persistence ------------------------------------------------------------------------

    /// Restores the previous session (active service, what was on screen, timers). Call once at
    /// start-up, after the repositories are open. Failures are logged and never fatal.
    pub fn restore(&self) {
        let raw = match self.repos.settings.get(SESSION_KEY) {
            Ok(Some(raw)) => raw,
            Ok(None) => return,
            Err(e) => {
                log::warn!("could not read the saved session: {e}");
                return;
            }
        };
        let saved: SavedSession = match serde_json::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("ignoring unreadable saved session: {e}");
                return;
            }
        };
        if let Some(pl) = &saved.active_playlist {
            if let Err(e) = self.set_active_playlist(pl) {
                log::warn!("could not reopen the last service: {e}");
            }
        }
        let restore = EngineCommand::Restore(Restore {
            live: saved.live,
            timers: saved.timers,
            service_mode: saved.service_mode,
        });
        if let Err(e) = self.engine.dispatch(&host_session(), None, restore).result {
            log::warn!("could not restore the previous screen: {e}");
        }
        self.host().last_saved = Some(raw);
    }

    fn persist(&self) {
        let mut host = self.host();
        self.persist_locked(&mut host);
    }

    /// Writes the session now (autosave tick, clean exit, panic hook). Unlike the per-command
    /// save it also captures state that changes without a command, such as running timers.
    /// Never panics and never blocks forever on a host lock held by a crashed thread.
    pub fn flush_session(&self) {
        let mut host = match self.host.try_lock() {
            Ok(g) => g,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        self.persist_locked(&mut host);
        drop(host);
        self.writer.drain(FLUSH_BUDGET);
    }

    /// Playlist currently loaded as the show, if any.
    pub fn active_playlist(&self) -> Option<PlaylistId> {
        self.host().active_playlist.clone()
    }

    /// Best effort: a failing disk must never stop a command that already succeeded.
    fn persist_locked(&self, host: &mut HostState) {
        let saved = self.engine.with_state(|s| SavedSession {
            active_playlist: host.active_playlist.clone(),
            live: s.live.clone(),
            timers: s.timers.clone(),
            service_mode: s.service_mode,
        });
        let Ok(json) = serde_json::to_string(&saved) else {
            return;
        };
        if host.last_saved.as_deref() == Some(json.as_str()) {
            return;
        }
        // Handed to the writer thread: the ack of a command never waits for the database.
        host.last_saved = Some(json.clone());
        self.writer.submit(json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{
        BibleRef, ItemContent, ParallelLayout, Playlist, ServiceItem, SlideKind, Target, TimerKind,
    };
    use hollyshow_engine::{ManualClock, SystemClock};
    use hollyshow_songs::DefaultSplitter;
    use hollyshow_storage_api::memory::MemoryStore;
    use hollyshow_storage_api::{PlaylistRepo, SettingsRepo, SongRepo, StorageResult};

    pub(super) struct Fixture {
        store: Arc<MemoryStore>,
        pub(super) app: AppService,
    }

    fn song(id: &str, lyrics: &str) -> hollyshow_core::Song {
        use hollyshow_core::*;
        Song {
            id: SongId::new(id),
            title: id.into(),
            alt_title: None,
            author: None,
            copyright: None,
            ccli: None,
            key: None,
            bpm: None,
            tags: vec![],
            theme_id: None,
            sections: vec![Section {
                id: SectionId::new("v1"),
                kind: SectionKind::Verse,
                label: "Verso 1".into(),
                lyrics: lyrics.into(),
                translation: None,
            }],
            arrangements: vec![],
            created_at: 0,
            updated_at: 0,
        }
    }

    fn playlist() -> Playlist {
        Playlist {
            id: PlaylistId::new("p1"),
            name: "Domingo".into(),
            date: None,
            is_template: false,
            items: vec![ServiceItem {
                id: ItemId::new("i1"),
                title: "Cancion".into(),
                content: ItemContent::Song {
                    song_id: "s1".into(),
                    arrangement_id: None,
                    transpose: 0,
                },
                duration_secs: None,
                theme_id: None,
                notes: None,
            }],
        }
    }

    fn fixture_with(repos_store: Arc<MemoryStore>, engine: Arc<Engine>) -> Fixture {
        let app = AppService::new(
            MemoryStore::bundle(repos_store.clone()),
            engine,
            Arc::new(DefaultSplitter),
        );
        Fixture {
            store: repos_store,
            app,
        }
    }

    pub(super) fn fixture() -> Fixture {
        let store = Arc::new(MemoryStore::new());
        SongRepo::save(&*store, &song("s1", "uno\ndos")).unwrap();
        PlaylistRepo::save(&*store, &playlist()).unwrap();
        fixture_with(store, Arc::new(Engine::default()))
    }

    fn op() -> ClientCtx {
        ClientCtx::local_operator()
    }

    fn install_bible(store: &MemoryStore) {
        store.install_version(
            BibleVersion {
                code: VersionCode("TEST".into()),
                name: "Test".into(),
                language: "es".into(),
                license: "PD".into(),
                copyright_text: "Dominio publico".into(),
                is_public_domain: true,
                versification: "kjv".into(),
                sort_order: 0,
                is_enabled: true,
                is_default: true,
                is_secondary: false,
            },
            (16..=17)
                .map(|n| Verse {
                    reference: BibleRef::verse("John", 3, n),
                    text: format!("texto {n}"),
                    marks: vec![],
                })
                .collect(),
        );
    }

    fn show_bible(osis: &str, version: &str) -> Command {
        Command::ShowBible {
            passage: Passage::parse_osis(osis).unwrap(),
            versions: vec![VersionCode(version.into())],
            layout: ParallelLayout::Columns,
            target: Target::Live,
        }
    }

    #[test]
    fn a_song_going_live_is_logged_once_per_service() {
        let f = fixture();
        assert!(
            f.app
                .dispatch(
                    &op(),
                    None,
                    Command::SetActivePlaylist {
                        id: PlaylistId::new("p1")
                    }
                )
                .ok
        );
        let live = || Command::ShowItem {
            item_id: ItemId::new("i1"),
            target: Target::Live,
        };
        assert!(f.app.dispatch(&op(), None, live()).ok);
        assert!(f.app.dispatch(&op(), None, live()).ok);
        let mut seen = 0;
        for _ in 0..100 {
            seen = f.store.song_usage().len();
            if seen > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(seen, 1);
        assert_eq!(f.store.song_usage().len(), 1);
        assert_eq!(f.store.song_usage()[0].0, hollyshow_core::SongId::new("s1"));
    }

    #[test]
    fn the_same_song_in_two_items_is_one_use() {
        let f = fixture();
        let mut pl = playlist();
        let mut second = pl.items[0].clone();
        second.id = ItemId::new("i2");
        pl.items.push(second);
        PlaylistRepo::save(&*f.store, &pl).unwrap();
        let live = |id: &str| Command::ShowItem {
            item_id: ItemId::new(id),
            target: Target::Live,
        };
        let open = Command::SetActivePlaylist {
            id: PlaylistId::new("p1"),
        };
        assert!(f.app.dispatch(&op(), None, open).ok);
        assert!(f.app.dispatch(&op(), None, live("i1")).ok);
        assert!(f.app.dispatch(&op(), None, live("i2")).ok);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(f.store.song_usage().len(), 1);
    }

    fn wait_usage(store: &MemoryStore, n: usize) {
        for _ in 0..100 {
            if store.song_usage().len() >= n {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    #[test]
    fn switching_playlists_back_and_forth_does_not_relog() {
        let f = fixture();
        let mut other = playlist();
        other.id = PlaylistId::new("p2");
        PlaylistRepo::save(&*f.store, &other).unwrap();
        let open = |id: &str| Command::SetActivePlaylist {
            id: PlaylistId::new(id),
        };
        let live = || Command::ShowItem {
            item_id: ItemId::new("i1"),
            target: Target::Live,
        };
        assert!(f.app.dispatch(&op(), None, open("p1")).ok);
        assert!(f.app.dispatch(&op(), None, live()).ok);
        assert!(f.app.dispatch(&op(), None, open("p2")).ok);
        assert!(f.app.dispatch(&op(), None, open("p1")).ok);
        assert!(f.app.dispatch(&op(), None, live()).ok);
        wait_usage(&f.store, 1);
        assert_eq!(f.store.song_usage().len(), 1);
    }

    #[test]
    fn a_restart_does_not_relog_a_song_shown_moments_ago() {
        let f = fixture();
        SongRepo::log_usage(
            &*f.store,
            &hollyshow_core::SongId::new("s1"),
            now_ms() - 1000,
        )
        .unwrap();
        // A fresh service over the same store, as after a restart.
        let app = AppService::new(
            MemoryStore::bundle(f.store.clone()),
            Arc::new(Engine::default()),
            Arc::new(DefaultSplitter),
        );
        assert!(
            app.dispatch(
                &op(),
                None,
                Command::SetActivePlaylist {
                    id: PlaylistId::new("p1")
                }
            )
            .ok
        );
        assert!(
            app.dispatch(
                &op(),
                None,
                Command::ShowItem {
                    item_id: ItemId::new("i1"),
                    target: Target::Live
                }
            )
            .ok
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(f.store.song_usage().len(), 1);
    }

    #[test]
    fn show_bible_is_resolved_then_applied() {
        let f = fixture();
        install_bible(&f.store);
        let ack = f
            .app
            .dispatch(&op(), None, show_bible("John.3.16-17", "TEST"));
        assert!(ack.ok, "{ack:?}");
        let live = f.app.engine().snapshot().live.unwrap();
        assert_eq!(live.slide.bible[0].text, "16 texto 16 17 texto 17");
        assert_eq!(
            live.slide.bible[0].copyright.as_deref(),
            Some("Dominio publico")
        );
    }

    #[test]
    fn unknown_version_and_oversized_requests_are_clean_errors() {
        let f = fixture();
        install_bible(&f.store);
        let ack = f.app.dispatch(&op(), None, show_bible("John.3.16", "NOPE"));
        assert!(!ack.ok);
        assert_eq!(ack.error.unwrap().code, "invalid");
        assert_eq!(f.app.engine().view().version, 0);
        let many = Command::ShowBible {
            passage: Passage::parse_osis("John.3.16").unwrap(),
            versions: vec![VersionCode("TEST".into()); 7],
            layout: ParallelLayout::Columns,
            target: Target::Live,
        };
        assert!(!f.app.dispatch(&op(), None, many).ok);
    }

    #[test]
    fn pastor_can_request_passages_but_nothing_else_reaches_the_projector() {
        let f = fixture();
        install_bible(&f.store);
        let pastor = ClientCtx::new(SessionId::new("p"), Role::Pastor);
        assert!(
            f.app
                .dispatch(&pastor, None, show_bible("John.3.16", "TEST"))
                .ok
        );
        for cmd in [
            Command::Blackout { on: true },
            Command::UpdateSlide {
                item_id: "x".into(),
                slide_index: 0,
                slide: Slide::blank("evil"),
            },
            Command::RefreshShow,
            Command::SetActivePlaylist { id: "p1".into() },
        ] {
            let ack = f.app.dispatch(&pastor, None, cmd);
            assert_eq!(ack.error.unwrap().code, "forbidden");
        }
        // A pastor request is only staged: the projector is untouched until the operator acts.
        let st = f.app.engine().snapshot();
        assert!(st.live.is_none());
        assert_eq!(st.preview.unwrap().slide.kind, SlideKind::Bible);
    }

    #[test]
    fn unknown_commands_are_unsupported_for_everyone() {
        let f = fixture();
        let ack = f.app.dispatch(&op(), None, Command::Unknown);
        assert_eq!(ack.error.unwrap().code, "unsupported");
    }

    #[test]
    fn active_playlist_expands_into_the_show() {
        let f = fixture();
        let ack = f
            .app
            .dispatch(&op(), None, Command::SetActivePlaylist { id: "p1".into() });
        assert!(ack.ok, "{ack:?}");
        let st = f.app.engine().snapshot();
        assert_eq!(st.show.len(), 1);
        assert_eq!(st.show[0].slides[0].lines, ["uno", "dos"]);
        let missing = f
            .app
            .dispatch(&op(), None, Command::SetActivePlaylist { id: "zz".into() });
        assert!(!missing.ok);
    }

    #[test]
    fn editing_a_song_refreshes_the_show_via_library_changed() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        SongRepo::save(&*f.store, &song("s1", "nuevo texto")).unwrap();
        let rev = f.app.engine().view().show_revision;
        f.app.library_changed(LibraryScope::Songs);
        let st = f.app.engine().snapshot();
        assert!(st.show_revision > rev);
        assert_eq!(st.show[0].slides[0].lines, ["nuevo texto"]);
    }

    #[test]
    fn live_edit_is_ephemeral_survives_refresh_and_never_touches_the_library() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        let mut edited = Slide::lyrics("ignored-id", None, &["corregido"]);
        edited.kind = SlideKind::Lyrics;
        let ack = f.app.dispatch(
            &op(),
            None,
            Command::UpdateSlide {
                item_id: "i1".into(),
                slide_index: 0,
                slide: edited,
            },
        );
        assert!(ack.ok, "{ack:?}");
        let id_before = f.app.engine().snapshot().show[0].slides[0].id.clone();
        assert_eq!(
            f.app.engine().snapshot().show[0].slides[0].lines,
            ["corregido"]
        );
        // Rebuilding the show keeps the edit on top.
        f.app.refresh_show().unwrap();
        let slide = f.app.engine().snapshot().show[0].slides[0].clone();
        assert_eq!(slide.lines, ["corregido"]);
        assert_eq!(slide.id, id_before, "slide id is pinned to its slot");
        // The song itself is untouched.
        let song = SongRepo::get(&*f.store, &"s1".into()).unwrap().unwrap();
        assert_eq!(song.sections[0].lyrics, "uno\ndos");
        // Opening another service (or this one again after a switch) drops the edit.
        let mut other = playlist();
        other.id = "p2".into();
        PlaylistRepo::save(&*f.store, &other).unwrap();
        f.app.set_active_playlist(&"p2".into()).unwrap();
        assert_eq!(
            f.app.engine().snapshot().show[0].slides[0].lines,
            ["uno", "dos"]
        );
    }

    #[test]
    fn session_is_persisted_and_restored_after_restart() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        assert!(
            f.app
                .dispatch(
                    &op(),
                    None,
                    Command::Next {
                        expected_version: None,
                        epoch: None
                    }
                )
                .ok
        );
        f.app
            .dispatch(&op(), None, Command::ServiceMode { on: true });
        let timer = Timer::new("svc", "Culto", TimerKind::Countdown, 600);
        f.app.dispatch(&op(), None, Command::SetTimer { timer });
        f.app.flush_session();
        assert!(SettingsRepo::get(&*f.store, SESSION_KEY).unwrap().is_some());

        // "Restart": a brand new engine and service over the same storage.
        let clock = Arc::new(ManualClock::new(0));
        let engine = Arc::new(Engine::new(Default::default(), clock));
        let g = fixture_with(f.store.clone(), engine);
        g.app.restore();
        let st = g.app.engine().snapshot();
        assert_eq!(st.show.len(), 1, "show rebuilt from the active playlist");
        assert_eq!(st.live.as_ref().unwrap().slide.lines, ["uno", "dos"]);
        assert!(st.service_mode);
        assert_eq!(st.timers.len(), 1);
    }

    #[test]
    fn corrupt_saved_session_is_ignored() {
        let f = fixture();
        SettingsRepo::set(&*f.store, SESSION_KEY, "{not json").unwrap();
        f.app.restore();
        assert_eq!(f.app.engine().view().version, 0);
    }

    #[test]
    fn queries_are_authorised_by_role() {
        let f = fixture();
        install_bible(&f.store);
        match f
            .app
            .query(&op(), &QueryRequest::ListBibleVersions)
            .unwrap()
        {
            QueryResponse::BibleVersions { items } => assert_eq!(items.len(), 1),
            other => panic!("{other:?}"),
        }
        let pastor = ClientCtx::new(SessionId::new("p"), Role::Pastor);
        assert!(f
            .app
            .query(&pastor, &QueryRequest::ListBibleVersions)
            .is_ok());
        let err = f
            .app
            .query(&pastor, &QueryRequest::ListPlaylists)
            .unwrap_err();
        assert_eq!(err.code, "forbidden");
        let err = f.app.query(&op(), &QueryRequest::Unknown).unwrap_err();
        assert_eq!(err.code, "unsupported");
    }

    struct PanickingPlaylists;
    impl PlaylistRepo for PanickingPlaylists {
        fn list(&self) -> StorageResult<Vec<hollyshow_core::PlaylistSummary>> {
            panic!("boom")
        }
        fn get(&self, _: &PlaylistId) -> StorageResult<Option<Playlist>> {
            panic!("boom")
        }
        fn save(&self, _: &Playlist) -> StorageResult<()> {
            Ok(())
        }
        fn delete(&self, _: &PlaylistId) -> StorageResult<()> {
            Ok(())
        }
    }

    #[test]
    fn a_panic_in_a_repository_is_contained_and_the_engine_survives() {
        let store = Arc::new(MemoryStore::new());
        let mut repos = MemoryStore::bundle(store);
        repos.playlists = Arc::new(PanickingPlaylists);
        let app = AppService::new(
            repos,
            Arc::new(Engine::new(Default::default(), Arc::new(SystemClock))),
            Arc::new(DefaultSplitter),
        );
        let ack = app.dispatch(&op(), None, Command::SetActivePlaylist { id: "p".into() });
        assert_eq!(ack.error.unwrap().code, "internal");
        let q = app.query(&op(), &QueryRequest::ListPlaylists).unwrap_err();
        assert_eq!(q.code, "internal");
        // Still alive and serving.
        assert!(app.dispatch(&op(), None, Command::Blackout { on: true }).ok);
        assert!(app.engine().view().blackout);
    }

    #[tokio::test]
    async fn lagging_client_gets_fresh_state_then_resync_and_roles_are_filtered() {
        let engine = Arc::new(Engine::with_capacity(
            Default::default(),
            Arc::new(SystemClock),
            2,
        ));
        let f = fixture_with(Arc::new(MemoryStore::new()), engine);
        let mut crowd = f.app.feed(Role::Congregation);
        let mut stage = f.app.feed(Role::Stage);
        for i in 0..6 {
            f.app
                .dispatch(&op(), None, Command::Blackout { on: i % 2 == 0 });
        }
        f.app.dispatch(
            &op(),
            None,
            Command::StageMessage {
                text: Some("secreto".into()),
            },
        );
        // Both feeds overflowed their 2-slot queue.
        let batch = crowd.next().await.unwrap();
        assert!(matches!(batch[0], ServerMsg::State { .. }));
        assert!(matches!(
            batch[1],
            ServerMsg::Resync {
                reason: hollyshow_core::ResyncReason::Lagged
            }
        ));
        if let ServerMsg::State { view, .. } = &batch[0] {
            assert!(
                view.stage_message.is_none(),
                "audience never sees stage notes"
            );
        }
        let batch = stage.next().await.unwrap();
        if let ServerMsg::State { view, .. } = &batch[0] {
            assert_eq!(view.stage_message.as_ref().unwrap().text, "secreto");
        }
        // After resync the feed first drains what is still queued, then delivers new updates.
        for _ in 0..2 {
            let drained = crowd.next().await.unwrap();
            assert!(matches!(drained[0], ServerMsg::State { .. }));
        }
        f.app.dispatch(&op(), None, Command::Logo { on: true });
        let next = crowd.next().await.unwrap();
        assert!(next.iter().any(|m| matches!(m, ServerMsg::Event { .. })));
    }

    #[test]
    fn snapshot_message_hides_the_show_from_the_audience() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        match f.app.snapshot_msg(Role::Congregation) {
            ServerMsg::Snapshot { snapshot, .. } => assert!(snapshot.show.is_none()),
            other => panic!("{other:?}"),
        }
        match f.app.snapshot_msg(Role::Operator) {
            ServerMsg::Snapshot { snapshot, .. } => assert_eq!(snapshot.show.unwrap().len(), 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn command_ids_are_scoped_per_session() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        let id = CommandId::new("c-1");
        let a = ClientCtx::new(SessionId::new("A"), Role::Operator);
        let b = ClientCtx::new(SessionId::new("B"), Role::Operator);
        let view = f.app.engine().view();
        let next = || Command::Next {
            expected_version: Some(view.version),
            epoch: Some(view.epoch),
        };
        let first = f.app.dispatch(&a, Some(&id), next());
        let retry = f.app.dispatch(&a, Some(&id), next());
        let other = f.app.dispatch(&b, Some(&id), next());
        assert!(!first.duplicate && retry.duplicate && !other.duplicate);
    }

    #[test]
    fn remote_relative_command_with_stale_epoch_is_refused() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        let remote = ClientCtx::new(SessionId::new("phone"), Role::Operator);
        let view = f.app.engine().view();
        let cmd = |epoch| Command::Next {
            expected_version: Some(view.version),
            epoch: Some(epoch),
        };
        let ack = f.app.dispatch(&remote, None, cmd(view.epoch ^ 1));
        assert!(!ack.ok);
        assert_eq!(ack.error.unwrap().code, "epoch_mismatch");
        assert_eq!(f.app.engine().view().version, view.version);
        assert!(f.app.dispatch(&remote, None, cmd(view.epoch)).ok);
    }

    #[test]
    fn remote_relative_commands_must_carry_expected_version() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        let remote = ClientCtx::new(SessionId::new("phone"), Role::Operator);
        for cmd in [
            Command::Next {
                expected_version: None,
                epoch: None,
            },
            Command::Prev {
                expected_version: None,
                epoch: None,
            },
        ] {
            let ack = f.app.dispatch(&remote, None, cmd.clone());
            assert!(!ack.ok);
            assert_eq!(ack.error.unwrap().code, "invalid");
        }
        let ack = f.app.dispatch(
            &ClientCtx::local_operator(),
            None,
            Command::Next {
                expected_version: None,
                epoch: None,
            },
        );
        assert_ne!(ack.error.map(|e| e.code), Some("invalid".to_string()));
    }
}
