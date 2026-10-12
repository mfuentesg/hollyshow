//! Application state shared by every Tauri command, the event bridge and the LAN server.
//!
//! [`AppState::open`] is the composition root: SQLite store (with corruption recovery and a daily
//! backup), the use-case layer, the image bank, the LAN server handle and the host-side helpers.

use crate::health::Health;
use crate::power::Power;
use crate::remote::Remote;
use crate::windows::WindowRegistry;
use hollyshow_app::AppService;
use hollyshow_core::{
    ItemContent, ItemId, Playlist, PlaylistId, ServiceItem, Theme, ThemeId, WireError,
};
use hollyshow_engine::Engine;
use hollyshow_media::DefaultMediaService;
use hollyshow_songs::DefaultSplitter;
use hollyshow_storage_api::{Repos, StorageError};
use hollyshow_storage_sqlite::{restore_backup, Store};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// How many daily backups are kept.
pub const BACKUPS_KEPT: usize = 14;
/// Rolling timestamped snapshots kept besides the daily ones.
pub const SNAPSHOTS_KEPT: usize = 5;
/// Settings key set to `1` while the app runs and to `0` on a clean exit (crash detection).
pub const RUNNING_KEY: &str = "session.running";
/// Settings key written once the starter content was created, so deleting it sticks.
const SEEDED_KEY: &str = "seeded.v1";

/// Where everything lives on disk.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub db: PathBuf,
    pub backups: PathBuf,
    pub media: PathBuf,
    /// Folders searched, in order, for the bundled `*.hsb.gz` Bibles.
    pub bible_dirs: Vec<PathBuf>,
}

impl Paths {
    pub fn new(data_dir: PathBuf, bible_dirs: Vec<PathBuf>) -> Self {
        Self {
            db: data_dir.join("hollyshow.db"),
            backups: data_dir.join("backups"),
            media: data_dir.join("media"),
            data_dir,
            bible_dirs,
        }
    }

    /// First candidate folder that exists and holds at least one Bible file.
    pub fn bundled_bibles(&self) -> Option<&Path> {
        self.bible_dirs
            .iter()
            .map(PathBuf::as_path)
            .find(|d| has_bible_files(d))
    }
}

fn has_bible_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten().any(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.ends_with(".hsb.gz") || n.ends_with(".hsb")
        })
    })
}

/// What happened to the database at start-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbRecovery {
    /// Opened normally.
    Clean,
    /// The file was unreadable; the newest good backup was restored (its date).
    RestoredBackup(String),
    /// The file was unreadable and no backup could be used: started with an empty library. The
    /// broken file was kept next to it.
    StartedEmpty,
}

/// What the operator window is told about this start-up ("retomar donde estabas").
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartupInfo {
    /// A previous session (open service, what was on screen) was restored.
    pub resumed: bool,
    /// The previous run did not exit cleanly (crash, power loss, kill).
    pub crashed: bool,
    /// Name of the service that was reopened.
    pub playlist: Option<String>,
    /// Something was on the projector when the app stopped.
    pub had_live: bool,
    /// The library had to be restored from a backup (or recreated).
    pub db_recovered: bool,
}

fn nonexistent_sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Opens the store; a corrupt file is set aside and replaced by the newest usable backup (or an
/// empty library). A database from a *newer* HollyShow is never touched: the error is returned.
pub fn open_store(paths: &Paths, now_ms: i64) -> Result<(Arc<Store>, DbRecovery), OpenError> {
    let attempt = Store::open(&paths.db).and_then(|s| s.integrity_check().map(|()| s));
    match attempt {
        Ok(store) => Ok((Arc::new(store), DbRecovery::Clean)),
        Err(StorageError::Invalid(m)) => Err(OpenError::Database(format!(
            "la base de datos no se puede usar: {m}"
        ))),
        Err(e) => {
            log::error!("library database unreadable ({e}); recovering");
            recover_unusable(paths, now_ms).map_err(OpenError::Other)
        }
    }
}

/// Sets the current database file aside (it is kept next to the new one) and starts from the
/// newest usable backup, or from an empty library when there is none.
pub fn recover_unusable(paths: &Paths, now_ms: i64) -> Result<(Arc<Store>, DbRecovery), String> {
    if paths.db.exists() {
        let broken = nonexistent_sibling(&paths.db, &format!(".corrupt-{now_ms}"));
        std::fs::rename(&paths.db, &broken)
            .map_err(|io| format!("no se pudo apartar la base: {io}"))?;
        for ext in ["-wal", "-shm"] {
            let side = nonexistent_sibling(&paths.db, ext);
            if side.exists() {
                let _ = std::fs::rename(&side, nonexistent_sibling(&broken, ext));
            }
        }
    }
    let backups = hollyshow_storage_sqlite::list_backup_files(&paths.backups);
    for b in backups {
        if restore_backup(&b.path, &paths.db).is_ok() {
            if let Ok(store) = Store::open(&paths.db) {
                log::warn!("library restored from backup {}", b.date);
                return Ok((Arc::new(store), DbRecovery::RestoredBackup(b.date)));
            }
        }
        let _ = std::fs::remove_file(&paths.db);
    }
    let store = Store::open(&paths.db).map_err(|e| format!("no se pudo crear la base: {e}"))?;
    Ok((Arc::new(store), DbRecovery::StartedEmpty))
}

/// Why the app could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The library database cannot be used (for instance it comes from a newer HollyShow). The
    /// user may choose to start from a backup or an empty library.
    Database(String),
    /// Anything else (unwritable data folder, image bank...).
    Other(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(m) | Self::Other(m) => f.write_str(m),
        }
    }
}

/// Starter content for a fresh install: the default theme and one service template.
pub fn seed_defaults(repos: &Repos) {
    if repos.settings.get(SEEDED_KEY).ok().flatten().is_some() {
        return;
    }
    if repos.themes.list().map(|t| t.is_empty()).unwrap_or(false) {
        let _ = repos.themes.save(&Theme::default_theme());
    }
    if repos
        .playlists
        .list()
        .map(|p| p.is_empty())
        .unwrap_or(false)
    {
        let _ = repos.playlists.save(&starter_template());
    }
    let _ = repos.settings.set(SEEDED_KEY, "1");
}

fn starter_template() -> Playlist {
    let item = |id: &str, title: &str, content: ItemContent, mins: u32| ServiceItem {
        id: ItemId::new(id),
        title: title.into(),
        content,
        duration_secs: (mins > 0).then_some(mins * 60),
        theme_id: None::<ThemeId>,
        notes: None,
    };
    Playlist {
        id: PlaylistId::new("template-culto"),
        name: "Culto dominical".into(),
        date: None,
        is_template: true,
        items: vec![
            item(
                "t-welcome",
                "Bienvenida",
                ItemContent::Text {
                    body: "Bienvenidos".into(),
                },
                3,
            ),
            item("t-worship", "Alabanza", ItemContent::Header, 25),
            item("t-offering", "Ofrenda", ItemContent::Header, 5),
            item("t-word", "Palabra", ItemContent::Header, 35),
            item(
                "t-close",
                "Despedida",
                ItemContent::Text {
                    body: "Que Dios les bendiga".into(),
                },
                2,
            ),
        ],
    }
}

#[derive(Clone)]
pub struct AppState {
    pub app: Arc<AppService>,
    pub store: Arc<Store>,
    pub media: Arc<DefaultMediaService>,
    pub paths: Paths,
    pub remote: Arc<Remote>,
    pub health: Arc<Health>,
    pub power: Arc<Power>,
    pub windows: Arc<WindowRegistry>,
    pub startup: StartupInfo,
}

impl AppState {
    /// Opens the library, restores the previous session and builds the services. Fast: the
    /// bundled Bibles are installed afterwards on a background thread (see `bibles`).
    pub fn open(paths: Paths) -> Result<Self, OpenError> {
        std::fs::create_dir_all(&paths.data_dir).map_err(|e| {
            OpenError::Other(format!(
                "no se pudo crear {}: {e}",
                paths.data_dir.display()
            ))
        })?;
        let now_ms = now_ms();
        let (store, recovery) = open_store(&paths, now_ms)?;
        if let Err(e) = store.daily_backup(&paths.backups, BACKUPS_KEPT, local_now_ms()) {
            log::warn!("daily backup failed: {e}");
        }
        let repos = store.repos();
        seed_defaults(&repos);

        let crashed = repos
            .settings
            .get(RUNNING_KEY)
            .ok()
            .flatten()
            .is_some_and(|v| v == "1");
        let _ = repos.settings.set(RUNNING_KEY, "1");

        let app = Arc::new(AppService::new(
            repos.clone(),
            Arc::new(Engine::default()),
            Arc::new(DefaultSplitter),
        ));
        app.restore();
        let view = app.engine().view();
        let playlist = app
            .active_playlist()
            .and_then(|id| repos.playlists.get(&id).ok().flatten().map(|p| p.name));
        let startup = StartupInfo {
            resumed: playlist.is_some() || view.live.is_some(),
            crashed,
            playlist,
            had_live: view.live.is_some(),
            db_recovered: recovery != DbRecovery::Clean,
        };

        let token = repos
            .settings
            .get("media.openverseToken")
            .ok()
            .flatten()
            .filter(|t| !t.trim().is_empty());
        let media = Arc::new(DefaultMediaService::open(&paths.media, token).map_err(|e| {
            OpenError::Other(format!("no se pudo abrir el banco de imagenes: {e}"))
        })?);

        #[cfg(debug_assertions)]
        load_demo_show(&app);

        let remote = Remote::new(app.clone(), repos.settings.clone(), paths.data_dir.clone());
        Ok(Self {
            app,
            store,
            media,
            paths,
            remote: Arc::new(remote),
            health: Arc::new(Health::new()),
            power: Arc::new(Power::default()),
            windows: Arc::new(WindowRegistry::default()),
            startup,
        })
    }

    /// Whether library edits are blocked ("modo culto").
    pub fn is_locked(&self) -> bool {
        self.app.is_locked()
    }

    /// Whether the machine must be kept awake right now (see [`crate::power::should_stay_awake`]).
    pub fn wants_awake(&self) -> bool {
        let view = self.app.engine().view();
        crate::power::should_stay_awake(
            view.service_mode,
            self.windows.any_open(),
            view.live.is_some(),
        )
    }

    /// Applies [`Self::wants_awake`] to the OS on a short-lived thread (the platform call may
    /// block). Idempotent; call it whenever a window opens or closes.
    pub fn sync_power(&self) {
        let state = self.clone();
        std::thread::spawn(move || {
            state.power.set_awake(state.wants_awake());
        });
    }

    /// Marks a clean exit and writes the session one last time.
    pub fn shutdown(&self) {
        self.app.flush_session();
        let _ = self.app.repos().settings.set(RUNNING_KEY, "0");
        // Last safety net: a snapshot of exactly what the user closed the app with.
        if let Err(e) =
            self.store
                .snapshot_backup(&self.paths.backups, SNAPSHOTS_KEPT, local_now_ms())
        {
            log::warn!("exit backup failed: {e}");
        }
    }
}

/// Wall-clock milliseconds shifted by the local UTC offset, so backup file names carry the local
/// date and time (an evening service must not roll over to "tomorrow" at UTC midnight).
pub fn local_now_ms() -> i64 {
    use chrono::Offset;
    let offset = chrono::Local::now().offset().fix().local_minus_utc();
    now_ms() + i64::from(offset) * 1000
}

/// Blocking jobs allowed at once. Keeps a burst of library work from filling the blocking pool
/// and starving the jobs that must stay responsive (autosave, backups, show control).
const BLOCKING_LIMIT: usize = 4;
static BLOCKING_GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(BLOCKING_LIMIT);

/// Runs blocking work (SQLite, files, `library_changed`) off the async workers, at most
/// [`BLOCKING_LIMIT`] jobs at a time. Async commands must use this instead of calling the
/// repositories inline.
pub async fn blocking<T, F>(f: F) -> Result<T, WireError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let internal = |m: String| WireError {
        code: "internal".into(),
        message: m,
    };
    let _permit = BLOCKING_GATE
        .acquire()
        .await
        .map_err(|e| internal(e.to_string()))?;
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| internal(e.to_string()))
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Debug builds on an empty show start with a few invented slides so a fresh checkout shows
/// something.
#[cfg(debug_assertions)]
fn load_demo_show(app: &AppService) {
    use hollyshow_core::{ShowEntry, Slide};
    if !app.engine().snapshot().show.is_empty() {
        return;
    }
    let entry = |id: &str, title: &str, slides: Vec<Slide>| ShowEntry {
        item_id: id.into(),
        title: title.into(),
        slides,
        duration_secs: None,
    };
    let entries = vec![entry(
        "demo-text",
        "Bienvenida",
        vec![Slide::lyrics("demo-text:0", None, &["Bienvenidos"])],
    )];
    let _ = app.load_entries(entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &Path) -> Paths {
        Paths::new(dir.to_path_buf(), vec![])
    }

    #[test]
    fn fresh_start_seeds_theme_and_template_once() {
        let dir = tempfile::tempdir().unwrap();
        let (store, rec) = open_store(&paths(dir.path()), 0).unwrap();
        assert_eq!(rec, DbRecovery::Clean);
        let repos = store.repos();
        seed_defaults(&repos);
        assert_eq!(repos.themes.list().unwrap().len(), 1);
        let pls = repos.playlists.list().unwrap();
        assert_eq!(pls.len(), 1);
        assert!(pls[0].is_template);
        // Deleting the template must stick.
        repos
            .playlists
            .delete(&PlaylistId::new("template-culto"))
            .unwrap();
        seed_defaults(&repos);
        assert!(repos.playlists.list().unwrap().is_empty());
    }

    #[test]
    fn corrupt_database_without_backup_starts_empty_and_keeps_the_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        std::fs::write(&p.db, b"this is definitely not a sqlite database").unwrap();
        let (store, rec) = open_store(&p, 7).unwrap();
        assert_eq!(rec, DbRecovery::StartedEmpty);
        assert!(store.integrity_check().is_ok());
        assert!(dir.path().join("hollyshow.db.corrupt-7").exists());
    }

    #[test]
    fn corrupt_database_is_restored_from_the_newest_backup() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        {
            let (store, _) = open_store(&p, 0).unwrap();
            seed_defaults(&store.repos());
            store
                .daily_backup(&p.backups, 5, 1_700_000_000_000)
                .unwrap()
                .expect("backup written");
        }
        std::fs::write(&p.db, b"garbage garbage garbage garbage").unwrap();
        for ext in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(nonexistent_sibling(&p.db, ext));
        }
        let (store, rec) = open_store(&p, 9).unwrap();
        assert!(matches!(rec, DbRecovery::RestoredBackup(_)), "{rec:?}");
        assert_eq!(store.repos().themes.list().unwrap().len(), 1);
    }

    #[test]
    fn startup_resumes_the_saved_service_and_detects_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        {
            let st = AppState::open(paths(dir.path())).unwrap();
            assert!(!st.startup.crashed);
            // Open the starter template as the service, then "crash" (no clean shutdown).
            let ack = st
                .app
                .set_active_playlist(&PlaylistId::new("template-culto"));
            assert!(ack.is_ok());
            st.app.flush_session();
        }
        let st = AppState::open(paths(dir.path())).unwrap();
        assert!(st.startup.crashed, "RUNNING_KEY was never cleared");
        assert!(st.startup.resumed);
        assert_eq!(st.startup.playlist.as_deref(), Some("Culto dominical"));
        st.shutdown();
        let st = AppState::open(paths(dir.path())).unwrap();
        assert!(!st.startup.crashed, "clean exit must clear the flag");
    }

    #[test]
    fn service_mode_locks_library_edits() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::open(paths(dir.path())).unwrap();
        assert!(st.app.ensure_unlocked().is_ok());
        let ack = st.app.dispatch(
            &hollyshow_app::ClientCtx::local_operator(),
            None,
            hollyshow_core::Command::ServiceMode { on: true },
        );
        assert!(ack.ok);
        assert_eq!(st.app.ensure_unlocked().unwrap_err().code, "locked");
    }
}
