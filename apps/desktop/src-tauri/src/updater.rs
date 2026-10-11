//! VS Code style auto update (PLAN §2 E3): `tauri-plugin-updater` + `tauri-plugin-process`.
//!
//! Flow: check 30 s after start and every 6 h -> download in the background (progress events) ->
//! `update-ready`. An update is **never installed while service mode is on or any output shows live
//! content**: it stays downloaded and is installed when the app exits ("instalar al salir").
//!
//! Integration (done by the shell owner, see `lib.rs`):
//! ```ignore
//! let builder = updater::plugins(tauri::Builder::default());   // updater + process plugins
//! ... .invoke_handler(generate_handler![ ..., updater::update_check, updater::update_download,
//!         updater::update_install_now, updater::update_status, updater::update_settings_get,
//!         updater::update_settings_set ])
//! ... .setup(|app| { updater::init(app.handle()); Ok(()) })
//! ... .build(tauri::generate_context!())?.run(|app, event| updater::on_run_event(app, &event));
//! ```
//!
//! Events (all payloads camelCase): `update-checking`, `update-available`, `update-progress`,
//! `update-ready`, `update-none`, `update-error`. TypeScript mirror: `src/operator/update/types.ts`.

use crate::state::AppState;
use hollyshow_core::StateView;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

/// First check after the app starts.
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
/// Interval between background checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Optional beta manifest (a `beta` pre-release holding its own `latest.json`). The stable endpoint
/// comes from `tauri.conf.json`.
pub const BETA_ENDPOINT: &str =
    "https://github.com/mfuentesg/hollyshow/releases/download/beta/latest.json";

const SETTINGS_FILE: &str = "updater.json";

// ---------------------------------------------------------------------------------------------
// Pure logic (unit tested)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    /// Endpoint override; `None` keeps the endpoints from `tauri.conf.json` (stable).
    pub fn endpoint(self) -> Option<&'static str> {
        match self {
            Channel::Stable => None,
            Channel::Beta => Some(BETA_ENDPOINT),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateSettings {
    pub auto_check: bool,
    pub auto_download: bool,
    pub channel: Channel,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: true,
            auto_download: true,
            channel: Channel::Stable,
        }
    }
}

/// Parses stored settings; anything unreadable falls back to the defaults (never blocks startup).
pub fn parse_settings(raw: &str) -> UpdateSettings {
    serde_json::from_str(raw).unwrap_or_default()
}

/// The app is "busy" (an update must not restart it) in service mode or when an enabled output is
/// showing live content that is not blacked out.
pub fn is_busy(view: &StateView) -> bool {
    if view.service_mode {
        return true;
    }
    let showing = view.live.is_some() && !view.blackout && !view.clear;
    showing
        && view.outputs.iter().any(|(id, look)| {
            look.enabled
                && !view
                    .output_layers
                    .get(id)
                    .is_some_and(|l| l.blackout && look.follows_blackout)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallDecision {
    /// Nothing was downloaded yet.
    NotReady,
    /// Busy: keep the package and install it on exit.
    DeferToExit,
    /// Safe to install and restart right now.
    InstallNow,
}

pub fn decide_install(ready: bool, busy: bool) -> InstallDecision {
    match (ready, busy) {
        (false, _) => InstallDecision::NotReady,
        (true, true) => InstallDecision::DeferToExit,
        (true, false) => InstallDecision::InstallNow,
    }
}

/// Whether a downloaded package must be installed when the app exits.
pub fn install_on_exit(ready: bool) -> bool {
    ready
}

/// Whether the scheduler should check on its own now.
pub fn should_auto_check(settings: &UpdateSettings, state: &State) -> bool {
    settings.auto_check && matches!(state, State::Idle | State::Error { .. })
}

/// Whether a found update should be downloaded without asking.
pub fn should_auto_download(settings: &UpdateSettings, manual: bool) -> bool {
    settings.auto_download || manual
}

// ---------------------------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    /// Release notes (Markdown of the tag). The UI renders it without raw HTML.
    pub notes: String,
    /// Publication date, epoch milliseconds.
    pub date: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum State {
    Idle,
    Checking,
    Available {
        #[serde(flatten)]
        info: ReleaseInfo,
    },
    Downloading {
        #[serde(flatten)]
        info: ReleaseInfo,
        downloaded: u64,
        total: Option<u64>,
    },
    Ready {
        #[serde(flatten)]
        info: ReleaseInfo,
        /// True when the app is busy: the update will be applied on exit.
        deferred: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    #[serde(flatten)]
    pub state: State,
    pub current_version: String,
    pub busy: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    version: String,
    downloaded: u64,
    total: Option<u64>,
}

// ---------------------------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------------------------

struct Inner {
    settings: UpdateSettings,
    state: State,
    /// Found update (needed to download), kept while `Available`/`Downloading`.
    update: Option<Update>,
    /// Downloaded package, installed now or on exit.
    package: Option<(Update, Vec<u8>)>,
}

pub struct UpdaterState {
    inner: Mutex<Inner>,
    settings_path: Mutex<Option<PathBuf>>,
}

impl UpdaterState {
    fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                settings: UpdateSettings::default(),
                state: State::Idle,
                update: None,
                package: None,
            }),
            settings_path: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Registers the plugins this module needs. Call before `.manage`/`.setup`.
pub fn plugins<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
}

/// Loads the settings and starts the background scheduler (first check after 30 s, then 6 h).
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let state = UpdaterState::new();
    if let Ok(dir) = app.path().app_config_dir() {
        let path = dir.join(SETTINGS_FILE);
        if let Ok(raw) = std::fs::read_to_string(&path) {
            state.lock().settings = parse_settings(&raw);
        }
        *state
            .settings_path
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(path);
    }
    app.manage(state);

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            let run = {
                let st = handle.state::<UpdaterState>();
                let g = st.lock();
                should_auto_check(&g.settings, &g.state)
            };
            if run {
                let _ = check(&handle, false).await;
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

fn busy_now<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<AppState>()
        .map(|s| is_busy(&s.app.engine().view()))
        .unwrap_or(false)
}

fn info_of(update: &Update) -> ReleaseInfo {
    ReleaseInfo {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        date: update.date.map(|d| d.unix_timestamp() * 1000),
    }
}

fn set_state<R: Runtime>(app: &AppHandle<R>, state: State) {
    app.state::<UpdaterState>().lock().state = state;
}

/// Checks the active channel; downloads when allowed. `manual` checks always download and report
/// "up to date" through `update-none`.
async fn check<R: Runtime>(app: &AppHandle<R>, manual: bool) -> Result<UpdateStatus, String> {
    let (settings, previous) = {
        let st = app.state::<UpdaterState>();
        let mut g = st.lock();
        match g.state {
            State::Checking | State::Downloading { .. } => {
                drop(g);
                return Ok(status(app));
            }
            // Already downloaded: nothing newer to look for until it is applied.
            State::Ready { .. } => {
                drop(g);
                return Ok(status(app));
            }
            _ => {}
        }
        let previous = g.state.clone();
        g.state = State::Checking;
        (g.settings.clone(), previous)
    };
    let _ = app.emit("update-checking", ());

    let found = async {
        let mut builder = app.updater_builder();
        if let Some(url) = settings.channel.endpoint() {
            let url: tauri::Url = url.parse().map_err(|e| format!("{e}"))?;
            builder = builder.endpoints(vec![url]).map_err(|e| e.to_string())?;
        }
        let updater = builder.build().map_err(|e| e.to_string())?;
        updater.check().await.map_err(|e| e.to_string())
    }
    .await;

    match found {
        Err(message) => {
            log::warn!("update check failed: {message}");
            set_state(
                app,
                State::Error {
                    message: message.clone(),
                },
            );
            let _ = app.emit("update-error", serde_json::json!({ "message": message }));
            Err(message)
        }
        Ok(None) => {
            let _ = previous;
            set_state(app, State::Idle);
            let _ = app.emit("update-none", serde_json::json!({ "manual": manual }));
            Ok(status(app))
        }
        Ok(Some(update)) => {
            let info = info_of(&update);
            {
                let st = app.state::<UpdaterState>();
                let mut g = st.lock();
                g.state = State::Available { info: info.clone() };
                g.update = Some(update);
            }
            let _ = app.emit("update-available", &info);
            if should_auto_download(&settings, manual) {
                download(app).await?;
            }
            Ok(status(app))
        }
    }
}

/// Downloads the found update in the background, emitting `update-progress`, then `update-ready`.
async fn download<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let (update, info) = {
        let st = app.state::<UpdaterState>();
        let mut g = st.lock();
        let Some(update) = g.update.take() else {
            return Ok(()); // nothing to download (or already downloading)
        };
        let info = info_of(&update);
        g.state = State::Downloading {
            info: info.clone(),
            downloaded: 0,
            total: None,
        };
        (update, info)
    };

    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    let progress_app = app.clone();
    let version = info.version.clone();
    let result = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                if last_emit.elapsed() >= Duration::from_millis(200) || Some(downloaded) == total {
                    last_emit = std::time::Instant::now();
                    if let Some(st) = progress_app.try_state::<UpdaterState>() {
                        st.lock().state = State::Downloading {
                            info: ReleaseInfo {
                                version: version.clone(),
                                notes: String::new(),
                                date: None,
                            },
                            downloaded,
                            total,
                        };
                    }
                    let _ = progress_app.emit(
                        "update-progress",
                        Progress {
                            version: version.clone(),
                            downloaded,
                            total,
                        },
                    );
                }
            },
            || {},
        )
        .await;

    match result {
        Ok(bytes) => {
            let deferred = busy_now(app);
            {
                let st = app.state::<UpdaterState>();
                let mut g = st.lock();
                g.state = State::Ready {
                    info: info.clone(),
                    deferred,
                };
                g.package = Some((update, bytes));
            }
            let _ = app.emit(
                "update-ready",
                serde_json::json!({
                    "version": info.version,
                    "notes": info.notes,
                    "date": info.date,
                    "deferred": deferred,
                }),
            );
            Ok(())
        }
        Err(e) => {
            let message = e.to_string();
            log::warn!("update download failed: {message}");
            set_state(
                app,
                State::Error {
                    message: message.clone(),
                },
            );
            let _ = app.emit("update-error", serde_json::json!({ "message": message }));
            Err(message)
        }
    }
}

fn status<R: Runtime>(app: &AppHandle<R>) -> UpdateStatus {
    let state = app.state::<UpdaterState>().lock().state.clone();
    UpdateStatus {
        state,
        current_version: app.package_info().version.to_string(),
        busy: busy_now(app),
    }
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

/// Manual "Buscar actualizaciones…": checks now and downloads if there is something new.
#[tauri::command]
pub async fn update_check<R: Runtime>(app: AppHandle<R>) -> Result<UpdateStatus, String> {
    check(&app, true).await
}

/// Starts the download of an update found with auto-download off.
#[tauri::command]
pub async fn update_download<R: Runtime>(app: AppHandle<R>) -> Result<UpdateStatus, String> {
    download(&app).await?;
    Ok(status(&app))
}

/// Current state, for windows opened after an event was emitted.
#[tauri::command]
pub fn update_status<R: Runtime>(app: AppHandle<R>) -> UpdateStatus {
    status(&app)
}

/// "Reiniciar y actualizar". Refuses (`busy`) during service mode or with a live output: the
/// update then waits for the app to exit.
#[tauri::command]
pub fn update_install_now<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let ready = app.state::<UpdaterState>().lock().package.is_some();
    match decide_install(ready, busy_now(&app)) {
        InstallDecision::NotReady => Err("not_ready".into()),
        InstallDecision::DeferToExit => {
            // Reflect the deferral so every window shows "Se instalará al salir".
            let st = app.state::<UpdaterState>();
            let mut g = st.lock();
            if let State::Ready { deferred, .. } = &mut g.state {
                *deferred = true;
            }
            Err("busy".into())
        }
        InstallDecision::InstallNow => {
            let package = app.state::<UpdaterState>().lock().package.take();
            let Some((update, bytes)) = package else {
                return Err("not_ready".into());
            };
            update.install(bytes).map_err(|e| e.to_string())?;
            app.restart()
        }
    }
}

#[tauri::command]
pub fn update_settings_get<R: Runtime>(app: AppHandle<R>) -> UpdateSettings {
    app.state::<UpdaterState>().lock().settings.clone()
}

#[tauri::command]
pub fn update_settings_set<R: Runtime>(
    app: AppHandle<R>,
    settings: UpdateSettings,
) -> Result<UpdateSettings, String> {
    let st = app.state::<UpdaterState>();
    let channel_changed = {
        let mut g = st.lock();
        let changed = g.settings.channel != settings.channel;
        g.settings = settings.clone();
        if changed {
            // A package from the other channel is no longer wanted.
            g.update = None;
            g.package = None;
            g.state = State::Idle;
        }
        changed
    };
    let path = st
        .settings_path
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if let Some(path) = path {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
    }
    if channel_changed {
        let _ = app.emit("update-none", serde_json::json!({ "manual": false }));
    }
    Ok(settings)
}

/// Installs a downloaded update when the app exits ("instalar al salir").
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: &tauri::RunEvent) {
    if !matches!(event, tauri::RunEvent::Exit) {
        return;
    }
    let Some(st) = app.try_state::<UpdaterState>() else {
        return;
    };
    let package = {
        let mut g = st.lock();
        if install_on_exit(g.package.is_some()) {
            g.package.take()
        } else {
            None
        }
    };
    if let Some((update, bytes)) = package {
        if let Err(e) = update.install(bytes) {
            log::error!("installing update on exit failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{PresentationState, Slide};

    fn view() -> StateView {
        PresentationState::default().view()
    }

    fn with_live(mut v: StateView) -> StateView {
        v.live = Some(hollyshow_core::Cue {
            item_id: None,
            slide_index: 0,
            slide: Slide::lyrics("s:0", None, &["x"]),
        });
        v
    }

    #[test]
    fn idle_app_is_not_busy() {
        assert!(!is_busy(&view()));
    }

    #[test]
    fn service_mode_is_busy() {
        let mut v = view();
        v.service_mode = true;
        assert!(is_busy(&v));
    }

    #[test]
    fn live_output_is_busy_until_blackout_or_clear() {
        let v = with_live(view());
        assert!(v.outputs.values().any(|o| o.enabled));
        assert!(is_busy(&v));
        let mut black = v.clone();
        black.blackout = true;
        assert!(!is_busy(&black));
        let mut cleared = v;
        cleared.clear = true;
        assert!(!is_busy(&cleared));
    }

    #[test]
    fn live_cue_with_every_output_disabled_is_not_busy() {
        let mut v = with_live(view());
        for o in v.outputs.values_mut() {
            o.enabled = false;
        }
        assert!(!is_busy(&v));
    }

    #[test]
    fn install_is_deferred_when_busy() {
        assert_eq!(decide_install(false, false), InstallDecision::NotReady);
        assert_eq!(decide_install(false, true), InstallDecision::NotReady);
        assert_eq!(decide_install(true, true), InstallDecision::DeferToExit);
        assert_eq!(decide_install(true, false), InstallDecision::InstallNow);
    }

    #[test]
    fn exit_installs_only_a_downloaded_package() {
        assert!(install_on_exit(true));
        assert!(!install_on_exit(false));
    }

    #[test]
    fn channels_pick_endpoints() {
        assert_eq!(Channel::Stable.endpoint(), None);
        assert_eq!(Channel::Beta.endpoint(), Some(BETA_ENDPOINT));
        assert!(BETA_ENDPOINT.parse::<tauri::Url>().is_ok());
    }

    #[test]
    fn auto_check_respects_settings_and_state() {
        let mut s = UpdateSettings::default();
        assert!(should_auto_check(&s, &State::Idle));
        assert!(should_auto_check(
            &s,
            &State::Error {
                message: "x".into()
            }
        ));
        assert!(!should_auto_check(&s, &State::Checking));
        s.auto_check = false;
        assert!(!should_auto_check(&s, &State::Idle));
    }

    #[test]
    fn manual_checks_always_download() {
        let s = UpdateSettings {
            auto_download: false,
            ..UpdateSettings::default()
        };
        assert!(!should_auto_download(&s, false));
        assert!(should_auto_download(&s, true));
        assert!(should_auto_download(&UpdateSettings::default(), false));
    }

    #[test]
    fn settings_round_trip_and_tolerate_garbage() {
        let s = UpdateSettings {
            auto_check: false,
            auto_download: true,
            channel: Channel::Beta,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"autoCheck\":false") && json.contains("\"channel\":\"beta\""));
        assert_eq!(parse_settings(&json), s);
        assert_eq!(parse_settings("{not json"), UpdateSettings::default());
        // Missing fields use defaults.
        assert_eq!(
            parse_settings("{\"channel\":\"beta\"}").channel,
            Channel::Beta
        );
        assert!(parse_settings("{\"channel\":\"beta\"}").auto_check);
    }

    #[test]
    fn status_serializes_flat_with_state_tag() {
        let st = UpdateStatus {
            state: State::Ready {
                info: ReleaseInfo {
                    version: "0.2.0".into(),
                    notes: "n".into(),
                    date: Some(1),
                },
                deferred: true,
            },
            current_version: "0.1.0".into(),
            busy: true,
        };
        let v = serde_json::to_value(&st).unwrap();
        assert_eq!(v["state"], "ready");
        assert_eq!(v["version"], "0.2.0");
        assert_eq!(v["deferred"], true);
        assert_eq!(v["currentVersion"], "0.1.0");
    }
}
