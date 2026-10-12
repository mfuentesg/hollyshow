//! Output / stage window management (PLAN §4 "Multi-monitor").
//!
//! * Windows are created at runtime so the operator can pick the screen. They are borderless,
//!   always on top and sized to the monitor, and never use native macOS fullscreen (it creates a
//!   separate Space and breaks mixed setups).
//! * The choice (which windows are open, on which monitor) is persisted and restored at start.
//! * Monitor hot-plug: when the monitor of a window disappears the output goes **black first**
//!   (its layers get `blackout`) and the window is parked; when a matching monitor returns the
//!   window moves back. The blackout is left on for the operator to release, so a flickering
//!   projector cable never flashes a half-ready slide at the congregation.

use crate::state::AppState;
use hollyshow_app::ClientCtx;
use hollyshow_core::{Command, MonitorInfo, OutputId, WireError};
use hollyshow_storage_api::SettingsRepo;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, Position, Size, State,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const PREFS_KEY: &str = "windows.v1";
pub const MONITORS_EVENT: &str = "hollyshow://monitors";
pub const CLOSE_BLOCKED_EVENT: &str = "hollyshow://close-blocked";
/// Emitted with the output ids that lost their monitor (payload: `string[]`).
pub const OUTPUT_PAUSED_EVENT: &str = "hollyshow://output-paused";
/// Emitted with the output ids whose window was just un-parked (placed on a monitor again), so
/// the output webviews resync right away instead of waiting for a focus/visibility change
/// (payload: `string[]`).
pub const OUTPUT_RESYNC_EVENT: &str = "hollyshow://output-resync";

fn err(code: &str, message: impl ToString) -> WireError {
    WireError {
        code: code.into(),
        message: message.to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// Pure logic
// ---------------------------------------------------------------------------------------------

/// Window label for an output id. Only `[a-z0-9-]` is accepted so it is safe in labels and URLs.
pub fn output_label(id: &str) -> Result<String, WireError> {
    let ok = !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(format!("output-{id}"))
    } else {
        Err(err(
            "invalid_output_id",
            format!("invalid output id `{id}`"),
        ))
    }
}

/// Output id a window label shows (`output-main` -> `main`, `stage` -> `stage`).
pub fn output_of_label(label: &str) -> Option<&str> {
    if label == "stage" {
        Some("stage")
    } else {
        label.strip_prefix("output-")
    }
}

/// Route of the SPA a window label shows.
pub fn route_of_label(label: &str) -> Option<String> {
    if label == "stage" {
        Some("/stage".into())
    } else {
        label
            .strip_prefix("output-")
            .map(|id| format!("/output/{id}"))
    }
}

/// Physical monitor rectangle to logical (x, y, w, h) for the window builder.
pub fn logical_rect(pos: (i32, i32), size: (u32, u32), scale: f64) -> (f64, f64, f64, f64) {
    let s = if scale > 0.0 { scale } else { 1.0 };
    (
        f64::from(pos.0) / s,
        f64::from(pos.1) / s,
        f64::from(size.0) / s,
        f64::from(size.1) / s,
    )
}

/// Identity of a monitor that survives re-ordering: name plus geometry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorKey {
    pub name: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl MonitorKey {
    pub fn of(m: &MonitorInfo) -> Self {
        Self {
            name: m.name.clone(),
            x: m.x,
            y: m.y,
            width: m.width,
            height: m.height,
        }
    }
}

/// Index of the monitor that best matches `key`, most specific first:
///
/// 1. same name, size and position;
/// 2. same name and size (the screen was moved in the display arrangement);
/// 3. same size and position, whatever the name (Windows renames `\\.\DISPLAY2` to
///    `\\.\DISPLAY3` when a projector is hot-plugged or the cable changes port);
/// 4. exactly one monitor of the same size that is not the operator's;
/// 5. exactly one monitor that is not the operator's, whatever it is.
///
/// `operator` is the monitor the operator window is on; it is never a fallback candidate. Steps 4
/// and 5 only apply when the choice is unambiguous, so an output never lands on a guess.
pub fn find_monitor(
    key: &MonitorKey,
    monitors: &[MonitorInfo],
    operator: Option<usize>,
) -> Option<usize> {
    let size_ok = |m: &MonitorInfo| m.width == key.width && m.height == key.height;
    let pos_ok = |m: &MonitorInfo| m.x == key.x && m.y == key.y;
    let same_name = |m: &MonitorInfo| key.name.is_some() && m.name == key.name;
    let only = |mut it: Box<dyn Iterator<Item = usize> + '_>| {
        let first = it.next()?;
        it.next().is_none().then_some(first)
    };
    // Without knowing which screen is the operator's, a guess could land on it.
    let others = || (0..monitors.len()).filter(move |i| operator.is_some_and(|op| op != *i));
    monitors
        .iter()
        .position(|m| same_name(m) && size_ok(m) && pos_ok(m))
        .or_else(|| monitors.iter().position(|m| same_name(m) && size_ok(m)))
        .or_else(|| monitors.iter().position(|m| size_ok(m) && pos_ok(m)))
        .or_else(|| only(Box::new(others().filter(|i| size_ok(&monitors[*i])))))
        .or_else(|| only(Box::new(others())))
}

/// Which monitor a window should open on: the explicit request, else the remembered one, else
/// the first monitor that does not host the operator window. `None` means "no second screen":
/// the window opens windowed instead of covering the operator.
pub fn choose_monitor(
    requested: Option<usize>,
    saved: Option<&MonitorKey>,
    monitors: &[MonitorInfo],
    operator: Option<usize>,
) -> Option<usize> {
    if let Some(i) = requested.filter(|i| *i < monitors.len()) {
        return Some(i);
    }
    if let Some(i) = saved.and_then(|k| find_monitor(k, monitors, operator)) {
        return Some(i);
    }
    (0..monitors.len()).find(|i| Some(*i) != operator)
}

/// Remembered state of one window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WinPref {
    pub open: bool,
    pub monitor: Option<MonitorKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Prefs {
    pub windows: BTreeMap<String, WinPref>,
}

impl Prefs {
    pub fn parse(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotplugAction {
    /// Open (or move) the window on this monitor.
    Place { label: String, monitor: usize },
    /// Its monitor is gone: black the output and hide the window.
    Park { label: String },
}

/// What to do for the current monitor set. `exists` tells whether the window currently exists,
/// `parked` whether it was hidden by an earlier unplug.
pub fn plan_hotplug(
    prefs: &Prefs,
    exists: impl Fn(&str) -> bool,
    parked: &HashSet<String>,
    monitors: &[MonitorInfo],
    operator: Option<usize>,
) -> Vec<HotplugAction> {
    let mut out = Vec::new();
    for (label, pref) in &prefs.windows {
        if !pref.open {
            continue;
        }
        let found = pref
            .monitor
            .as_ref()
            .and_then(|k| find_monitor(k, monitors, operator));
        match (found, exists(label)) {
            (Some(monitor), true) => {
                // Only touch windows that are parked (monitor came back) so a window the
                // operator dragged elsewhere is not yanked around on every tick.
                if parked.contains(label) {
                    out.push(HotplugAction::Place {
                        label: label.clone(),
                        monitor,
                    });
                }
            }
            (Some(monitor), false) => out.push(HotplugAction::Place {
                label: label.clone(),
                monitor,
            }),
            (None, true) => {
                if pref.monitor.is_some() && !parked.contains(label) {
                    out.push(HotplugAction::Park {
                        label: label.clone(),
                    });
                }
            }
            (None, false) => {}
        }
    }
    out
}

/// Closing the operator window ends the app, except while service mode is on (a stray click on
/// the close button must not kill the projector).
pub fn may_close_operator(service_mode: bool) -> bool {
    !service_mode
}

/// Output and stage windows cannot be closed by Alt+F4 / Cmd+W while service mode is on (the
/// operator closes them from the app, which is deliberate).
pub fn may_close_output(service_mode: bool) -> bool {
    !service_mode
}

/// Whether the app may quit. `code` is `None` when the user asked (Cmd+Q, the app-menu Quit, the
/// last window closing) and `Some` when the app itself exits or restarts (an update, the
/// operator window closing outside service mode); only the former is blocked in service mode.
pub fn may_exit(service_mode: bool, code: Option<i32>) -> bool {
    !service_mode || code.is_some()
}

// ---------------------------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct WindowRegistry {
    prefs: Mutex<Prefs>,
    parked: Mutex<HashSet<String>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl WindowRegistry {
    pub fn load(&self, settings: &dyn SettingsRepo) {
        if let Ok(Some(raw)) = settings.get(PREFS_KEY) {
            *lock(&self.prefs) = Prefs::parse(&raw);
        }
    }

    fn save(&self, settings: &dyn SettingsRepo) {
        if let Ok(json) = serde_json::to_string(&*lock(&self.prefs)) {
            if let Err(e) = settings.set(PREFS_KEY, &json) {
                log::warn!("could not persist window layout: {e}");
            }
        }
    }

    pub fn prefs(&self) -> Prefs {
        lock(&self.prefs).clone()
    }

    fn remember(&self, settings: &dyn SettingsRepo, label: &str, pref: WinPref) {
        lock(&self.prefs).windows.insert(label.to_owned(), pref);
        self.save(settings);
    }

    fn forget_open(&self, settings: &dyn SettingsRepo, label: &str) {
        if let Some(p) = lock(&self.prefs).windows.get_mut(label) {
            p.open = false;
        }
        lock(&self.parked).remove(label);
        self.save(settings);
    }

    fn set_parked(&self, label: &str, parked: bool) {
        let mut g = lock(&self.parked);
        if parked {
            g.insert(label.to_owned());
        } else {
            g.remove(label);
        }
    }

    /// Whether any output or stage window is meant to be open (including one parked because its
    /// monitor is away: it comes back and must find the display awake).
    pub fn any_open(&self) -> bool {
        lock(&self.prefs).windows.values().any(|p| p.open)
    }

    pub fn parked(&self) -> HashSet<String> {
        lock(&self.parked).clone()
    }
}

fn monitor_info(index: usize, m: &Monitor, primary: Option<&Monitor>) -> MonitorInfo {
    MonitorInfo {
        index: index as u32,
        name: m.name().cloned(),
        x: m.position().x,
        y: m.position().y,
        width: m.size().width,
        height: m.size().height,
        scale_factor: m.scale_factor(),
        is_primary: primary.is_some_and(|p| p.position() == m.position() && p.size() == m.size()),
    }
}

pub fn monitors_of(app: &AppHandle) -> Result<Vec<(MonitorInfo, Monitor)>, WireError> {
    let primary = app.primary_monitor().map_err(|e| err("monitor", e))?;
    let all = app.available_monitors().map_err(|e| err("monitor", e))?;
    Ok(all
        .into_iter()
        .enumerate()
        .map(|(i, m)| (monitor_info(i, &m, primary.as_ref()), m))
        .collect())
}

/// Index of the monitor the operator window is on.
fn operator_monitor(app: &AppHandle, infos: &[MonitorInfo]) -> Option<usize> {
    let w = app.get_webview_window("main")?;
    let cur = w.current_monitor().ok().flatten()?;
    infos.iter().position(|m| {
        m.x == cur.position().x && m.y == cur.position().y && m.width == cur.size().width
    })
}

fn place(w: &WebviewWindow, m: &Monitor) -> Result<(), WireError> {
    w.set_position(Position::Physical(PhysicalPosition::new(
        m.position().x,
        m.position().y,
    )))
    .map_err(|e| err("window", e))?;
    w.set_size(Size::Physical(PhysicalSize::new(
        m.size().width,
        m.size().height,
    )))
    .map_err(|e| err("window", e))
}

/// Creates the window `label` (or moves the existing one) on `monitor`. Without a monitor it is
/// a small windowed preview.
fn open_window(app: &AppHandle, label: &str, monitor: Option<&Monitor>) -> Result<(), WireError> {
    let route = route_of_label(label).ok_or_else(|| err("invalid_window", label))?;
    if let Some(w) = app.get_webview_window(label) {
        if let Some(m) = monitor {
            place(&w, m)?;
        }
        w.show().map_err(|e| err("window", e))?;
        return Ok(());
    }
    let title = if label == "stage" {
        "HollyShow - Escenario"
    } else {
        "HollyShow - Salida"
    };
    let mut b = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("index.html#{route}").into()),
    )
    .title(title)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(monitor.is_some())
    .resizable(monitor.is_none())
    .shadow(false)
    .focused(false);
    b = match monitor {
        Some(m) => {
            let (x, y, w, h) = logical_rect(
                (m.position().x, m.position().y),
                (m.size().width, m.size().height),
                m.scale_factor(),
            );
            b.position(x, y).inner_size(w, h)
        }
        None => b.inner_size(960.0, 540.0),
    };
    let w = b.build().map_err(|e| err("window", e))?;
    // The logical rectangle is rounded differently per platform; the physical one is exact.
    if let Some(m) = monitor {
        place(&w, m)?;
        let _ = w.set_cursor_visible(false);
    }
    Ok(())
}

/// Black out the output shown by `label` (if it exists) before its window goes away.
fn black_out(app: &AppHandle, label: &str) {
    let Some(output) = output_of_label(label) else {
        return;
    };
    let state = app.state::<AppState>();
    let id = OutputId::new(output);
    let view = state.app.engine().view();
    let Some(layers) = view.output_layers.get(&id) else {
        return;
    };
    if layers.blackout {
        return;
    }
    let mut layers = layers.clone();
    layers.blackout = true;
    let ack = state.app.dispatch(
        &ClientCtx::local_operator(),
        None,
        Command::SetOutputLayers { output: id, layers },
    );
    if !ack.ok {
        log::warn!("could not black out {output} after its monitor vanished: {ack:?}");
    }
}

/// Output ids behind the given window labels (each once, in order; non-output labels dropped).
pub fn output_ids_of_labels(labels: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for id in labels.iter().filter_map(|l| output_of_label(l)) {
        if !ids.iter().any(|i| i == id) {
            ids.push(id.to_owned());
        }
    }
    ids
}

/// Tells the output webviews that these windows were un-parked so they resync their state.
/// Nothing is emitted when no output is concerned.
pub fn emit_output_resync<R: tauri::Runtime>(app: &AppHandle<R>, labels: &[String]) {
    let ids = output_ids_of_labels(labels);
    if !ids.is_empty() {
        let _ = app.emit(OUTPUT_RESYNC_EVENT, &ids);
    }
}

/// Applies the hot-plug plan for the current monitors.
pub fn reconcile(app: &AppHandle) -> Result<(), WireError> {
    let state = app.state::<AppState>();
    let mons = monitors_of(app)?;
    let infos: Vec<MonitorInfo> = mons.iter().map(|(i, _)| i.clone()).collect();
    let prefs = state.windows.prefs();
    let parked = state.windows.parked();
    let plan = plan_hotplug(
        &prefs,
        |l| app.get_webview_window(l).is_some(),
        &parked,
        &infos,
        operator_monitor(app, &infos),
    );
    let mut paused = Vec::new();
    let mut unparked = Vec::new();
    for action in plan {
        match action {
            HotplugAction::Park { label } => {
                // Black first, then hide: the congregation never sees the slide jump to the
                // operator's screen.
                black_out(app, &label);
                if let Some(w) = app.get_webview_window(&label) {
                    let _ = w.hide();
                }
                state.windows.set_parked(&label, true);
                log::warn!("monitor of window {label} disappeared: output blacked out");
                if let Some(id) = output_of_label(&label) {
                    paused.push(id.to_owned());
                }
            }
            HotplugAction::Place { label, monitor } => {
                if let Some((_, m)) = mons.get(monitor) {
                    match open_window(app, &label, Some(m)) {
                        Ok(()) => {
                            state.windows.set_parked(&label, false);
                            log::info!("window {label} placed on monitor {monitor}");
                            unparked.push(label.clone());
                        }
                        Err(e) => log::warn!("could not place {label}: {}", e.message),
                    }
                }
            }
        }
    }
    if !paused.is_empty() {
        let _ = app.emit(OUTPUT_PAUSED_EVENT, &paused);
    }
    emit_output_resync(app, &unparked);
    let _ = app.emit(MONITORS_EVENT, &infos);
    Ok(())
}

/// Reopens the windows the operator had open last time. Called once at start-up.
pub fn restore_windows(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.windows.load(state.app.repos().settings.as_ref());
    if let Err(e) = reconcile(app) {
        log::warn!("could not restore output windows: {}", e.message);
    }
    // Windows that were open as small previews (single-screen setups) have no monitor to match.
    for (label, pref) in state.windows.prefs().windows {
        if pref.open && pref.monitor.is_none() && app.get_webview_window(&label).is_none() {
            if let Err(e) = open_window(app, &label, None) {
                log::warn!("could not reopen {label}: {}", e.message);
            }
        }
    }
}

/// Watches for monitors being plugged or unplugged (Tauri has no event for it).
pub fn watch_monitors(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<Vec<MonitorInfo>> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let Ok(mons) = monitors_of(&app) else {
                continue;
            };
            let infos: Vec<MonitorInfo> = mons.into_iter().map(|(i, _)| i).collect();
            if last.as_ref() != Some(&infos) {
                if last.is_some() {
                    let _ = reconcile(&app);
                }
                last = Some(infos);
            }
        }
    });
}

fn open_command(
    app: &AppHandle,
    state: &AppState,
    label: &str,
    monitor: Option<usize>,
) -> Result<(), WireError> {
    let mons = monitors_of(app)?;
    let infos: Vec<MonitorInfo> = mons.iter().map(|(i, _)| i.clone()).collect();
    if let Some(i) = monitor {
        if i >= infos.len() {
            return Err(err("monitor", format!("no monitor {i}")));
        }
    }
    let saved = state
        .windows
        .prefs()
        .windows
        .get(label)
        .and_then(|p| p.monitor.clone());
    let chosen = choose_monitor(
        monitor,
        saved.as_ref(),
        &infos,
        operator_monitor(app, &infos),
    );
    let was_parked = state.windows.parked().contains(label);
    open_window(app, label, chosen.and_then(|i| mons.get(i)).map(|(_, m)| m))?;
    state.windows.set_parked(label, false);
    if was_parked {
        emit_output_resync(app, &[label.to_owned()]);
    }
    state.windows.remember(
        state.app.repos().settings.as_ref(),
        label,
        WinPref {
            open: true,
            monitor: chosen.and_then(|i| infos.get(i)).map(MonitorKey::of),
        },
    );
    state.sync_power();
    Ok(())
}

#[tauri::command(async)]
pub fn open_output(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    monitor: Option<usize>,
) -> Result<(), WireError> {
    let label = output_label(&id)?;
    open_command(&app, &state, &label, monitor)
}

#[tauri::command(async)]
pub fn open_stage(
    app: AppHandle,
    state: State<'_, AppState>,
    monitor: Option<usize>,
) -> Result<(), WireError> {
    open_command(&app, &state, "stage", monitor)
}

#[tauri::command(async)]
pub fn close_window(
    app: AppHandle,
    state: State<'_, AppState>,
    label: String,
) -> Result<(), WireError> {
    // The operator window can never be closed through this command.
    if label == "main" {
        return Err(err("forbidden", "cannot close the operator window"));
    }
    state
        .windows
        .forget_open(state.app.repos().settings.as_ref(), &label);
    let res = match app.get_webview_window(&label) {
        Some(w) => w.destroy().map_err(|e| err("window", e)),
        None => Ok(()),
    };
    state.sync_power();
    res
}

#[tauri::command(async)]
pub fn list_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, WireError> {
    Ok(monitors_of(&app)?.into_iter().map(|(i, _)| i).collect())
}

/// Monitor index the operator window is on (the "Esta pantalla" choice), if it can be told.
#[tauri::command(async)]
pub fn operator_monitor_index(app: AppHandle) -> Result<Option<usize>, WireError> {
    let infos: Vec<MonitorInfo> = monitors_of(&app)?.into_iter().map(|(i, _)| i).collect();
    Ok(operator_monitor(&app, &infos))
}

/// Monitor index each open output window uses (`null` when it is windowed or parked).
#[tauri::command(async)]
pub fn output_monitors(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BTreeMap<String, Option<usize>>, WireError> {
    let infos: Vec<MonitorInfo> = monitors_of(&app)?.into_iter().map(|(i, _)| i).collect();
    let parked = state.windows.parked();
    let operator = operator_monitor(&app, &infos);
    let mut out = BTreeMap::new();
    for (label, pref) in state.windows.prefs().windows {
        let open = app.get_webview_window(&label).is_some() || parked.contains(&label);
        if !pref.open || !open {
            continue;
        }
        let Some(id) = output_of_label(&label) else {
            continue;
        };
        let idx = pref
            .monitor
            .as_ref()
            .and_then(|k| find_monitor(k, &infos, operator))
            .filter(|_| !parked.contains(&label));
        out.insert(id.to_owned(), idx);
    }
    Ok(out)
}

/// Window events: the operator window closes the app (unless service mode locks it) and a user
/// closing an output window by hand is remembered.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let app = window.app_handle();
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if window.label() == "main" {
        if may_close_operator(state.is_locked()) {
            app.exit(0);
        } else {
            api.prevent_close();
            let _ = app.emit(CLOSE_BLOCKED_EVENT, ());
        }
    } else if output_of_label(window.label()).is_some() {
        if may_close_output(state.is_locked()) {
            state
                .windows
                .forget_open(state.app.repos().settings.as_ref(), window.label());
            state.sync_power();
        } else {
            // A stray Alt+F4 / Cmd+W must not drop the projector in the middle of a service.
            api.prevent_close();
            let _ = app.emit(CLOSE_BLOCKED_EVENT, ());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(i: u32, name: &str, x: i32, w: u32, h: u32) -> MonitorInfo {
        MonitorInfo {
            index: i,
            name: Some(name.into()),
            x,
            y: 0,
            width: w,
            height: h,
            scale_factor: 1.0,
            is_primary: i == 0,
        }
    }

    #[test]
    fn resync_ids_come_from_output_labels_once() {
        let labels: Vec<String> = ["output-main", "stage", "output-main", "main", "output-aux1"]
            .map(String::from)
            .into();
        assert_eq!(
            output_ids_of_labels(&labels),
            vec!["main".to_string(), "stage".into(), "aux1".into()]
        );
        assert!(output_ids_of_labels(&["main".to_string()]).is_empty());
    }

    #[test]
    fn unparking_emits_the_resync_event_with_output_ids() {
        use std::sync::{Arc, Mutex};
        use tauri::Listener;
        let app = tauri::test::mock_app();
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = seen.clone();
        app.listen(OUTPUT_RESYNC_EVENT, move |e| {
            sink.lock().unwrap().push(e.payload().to_owned());
        });
        // Nothing to resync: no event.
        emit_output_resync(app.handle(), &[]);
        emit_output_resync(app.handle(), &["main".to_string()]);
        emit_output_resync(
            app.handle(),
            &["output-main".to_string(), "output-aux1".to_string()],
        );
        // Delivery is synchronous on the mock runtime, but give the dispatcher a moment.
        for _ in 0..50 {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            *seen.lock().unwrap(),
            vec![r#"["main","aux1"]"#.to_string()]
        );
    }

    #[test]
    fn labels_are_restricted() {
        assert_eq!(output_label("main").unwrap(), "output-main");
        assert_eq!(output_label("lobby-2").unwrap(), "output-lobby-2");
        for bad in ["", "Main", "a b", "../x", "a/b", &"x".repeat(40)] {
            assert!(output_label(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn label_and_route_round_trip() {
        assert_eq!(output_of_label("output-aux1"), Some("aux1"));
        assert_eq!(output_of_label("stage"), Some("stage"));
        assert_eq!(output_of_label("main"), None);
        assert_eq!(
            route_of_label("output-aux1").as_deref(),
            Some("/output/aux1")
        );
        assert_eq!(route_of_label("stage").as_deref(), Some("/stage"));
        assert_eq!(route_of_label("main"), None);
    }

    #[test]
    fn retina_monitor_rect_is_logical() {
        assert_eq!(
            logical_rect((2560, 0), (3840, 2160), 2.0),
            (1280.0, 0.0, 1920.0, 1080.0)
        );
        assert_eq!(
            logical_rect((-1920, 0), (1920, 1080), 1.0),
            (-1920.0, 0.0, 1920.0, 1080.0)
        );
        assert_eq!(
            logical_rect((0, 0), (100, 100), 0.0),
            (0.0, 0.0, 100.0, 100.0)
        );
    }

    #[test]
    fn monitors_match_by_name_and_size_even_when_reordered() {
        let laptop = mon(0, "eDP-1", 0, 1920, 1080);
        let proj = mon(1, "HDMI-1", 1920, 1920, 1080);
        let key = MonitorKey::of(&proj);
        assert_eq!(
            find_monitor(&key, &[laptop.clone(), proj.clone()], Some(0)),
            Some(1)
        );
        // Plugged back first in the list and shifted in the arrangement.
        let moved = mon(0, "HDMI-1", -1920, 1920, 1080);
        assert_eq!(
            find_monitor(&key, &[moved, laptop.clone()], Some(1)),
            Some(0)
        );
        // Same name but another resolution is a different screen.
        let other = mon(0, "HDMI-1", 1920, 1280, 720);
        assert_eq!(find_monitor(&key, &[laptop, other], None), None);
    }

    #[test]
    fn a_renamed_windows_display_is_still_found() {
        let laptop = mon(0, r"\\.\DISPLAY1", 0, 1920, 1080);
        let proj = mon(1, r"\\.\DISPLAY2", 1920, 1920, 1080);
        let key = MonitorKey::of(&proj);
        // Same geometry, new GDI name.
        let renamed = mon(1, r"\\.\DISPLAY3", 1920, 1920, 1080);
        assert_eq!(
            find_monitor(&key, &[laptop.clone(), renamed], Some(0)),
            Some(1)
        );
        // Renamed and re-arranged: the only other screen of that resolution.
        let moved = mon(1, r"\\.\DISPLAY3", -1920, 1920, 1080);
        assert_eq!(
            find_monitor(&key, &[laptop.clone(), moved], Some(0)),
            Some(1)
        );
        // Renamed and a different resolution: still the only non-operator screen.
        let tv = mon(1, r"\\.\DISPLAY3", 1920, 3840, 2160);
        assert_eq!(
            find_monitor(&key, &[laptop.clone(), tv.clone()], Some(0)),
            Some(1)
        );
        // Never the operator's own screen, and never a guess between several candidates.
        assert_eq!(
            find_monitor(&key, std::slice::from_ref(&laptop), Some(0)),
            None
        );
        let tv2 = mon(2, "HDMI-2", 5760, 3840, 2160);
        assert_eq!(
            find_monitor(&key, &[laptop.clone(), tv, tv2], Some(0)),
            None
        );
        // Without knowing the operator screen there is no fallback at all.
        let other = mon(1, r"\\.\DISPLAY3", 1920, 1280, 720);
        assert_eq!(find_monitor(&key, &[laptop, other], None), None);
    }

    #[test]
    fn a_renamed_monitor_comes_back_through_the_hotplug_plan() {
        let laptop = mon(0, r"\\.\DISPLAY1", 0, 1920, 1080);
        let proj = mon(1, r"\\.\DISPLAY2", 1920, 1920, 1080);
        let prefs = prefs_with("output-main", &proj);
        let parked: HashSet<String> = ["output-main".to_string()].into();
        let back = mon(1, r"\\.\DISPLAY3", 1920, 1920, 1080);
        assert_eq!(
            plan_hotplug(&prefs, |_| true, &parked, &[laptop, back], Some(0)),
            vec![HotplugAction::Place {
                label: "output-main".into(),
                monitor: 1
            }]
        );
    }

    #[test]
    fn service_mode_blocks_user_exits_but_not_the_apps_own() {
        assert!(!may_exit(true, None));
        assert!(may_exit(true, Some(0)), "updates and explicit exits pass");
        assert!(may_exit(false, None));
        assert!(!may_close_output(true));
        assert!(may_close_output(false));
    }

    #[test]
    fn unnamed_monitors_need_identical_geometry() {
        let mut a = mon(0, "x", 100, 800, 600);
        a.name = None;
        let key = MonitorKey::of(&a);
        assert_eq!(find_monitor(&key, &[a.clone()], None), Some(0));
        let mut b = a.clone();
        b.x = 200;
        assert_eq!(find_monitor(&key, &[b], None), None);
    }

    #[test]
    fn automatic_choice_skips_the_operator_screen() {
        let ms = [
            mon(0, "eDP-1", 0, 1920, 1080),
            mon(1, "HDMI-1", 1920, 1920, 1080),
        ];
        assert_eq!(choose_monitor(None, None, &ms, Some(0)), Some(1));
        assert_eq!(choose_monitor(None, None, &ms, Some(1)), Some(0));
        // Single screen: never cover the operator.
        assert_eq!(choose_monitor(None, None, &ms[..1], Some(0)), None);
        // Explicit request wins, a stale one falls back.
        assert_eq!(choose_monitor(Some(0), None, &ms, Some(0)), Some(0));
        assert_eq!(choose_monitor(Some(9), None, &ms, Some(0)), Some(1));
        // Remembered monitor wins over automatic.
        let saved = MonitorKey::of(&ms[0]);
        assert_eq!(choose_monitor(None, Some(&saved), &ms, Some(1)), Some(0));
    }

    fn prefs_with(label: &str, m: &MonitorInfo) -> Prefs {
        let mut p = Prefs::default();
        p.windows.insert(
            label.into(),
            WinPref {
                open: true,
                monitor: Some(MonitorKey::of(m)),
            },
        );
        p
    }

    #[test]
    fn unplugging_parks_the_window_and_replugging_places_it_again() {
        let laptop = mon(0, "eDP-1", 0, 1920, 1080);
        let proj = mon(1, "HDMI-1", 1920, 1920, 1080);
        let prefs = prefs_with("output-main", &proj);
        let mut parked = HashSet::new();

        // Everything connected: nothing to do.
        let both = [laptop.clone(), proj.clone()];
        assert!(plan_hotplug(&prefs, |_| true, &parked, &both, Some(0)).is_empty());

        // Projector unplugged: park (once).
        let only_laptop = [laptop.clone()];
        let plan = plan_hotplug(&prefs, |_| true, &parked, &only_laptop, Some(0));
        assert_eq!(
            plan,
            vec![HotplugAction::Park {
                label: "output-main".into()
            }]
        );
        parked.insert("output-main".to_string());
        assert!(plan_hotplug(&prefs, |_| true, &parked, &only_laptop, Some(0)).is_empty());

        // Plugged back: move the parked window onto it.
        let plan = plan_hotplug(&prefs, |_| true, &parked, &both, Some(0));
        assert_eq!(
            plan,
            vec![HotplugAction::Place {
                label: "output-main".into(),
                monitor: 1
            }]
        );
    }

    #[test]
    fn saved_windows_open_at_start_only_when_their_monitor_exists() {
        let laptop = mon(0, "eDP-1", 0, 1920, 1080);
        let proj = mon(1, "HDMI-1", 1920, 1920, 1080);
        let prefs = prefs_with("stage", &proj);
        let none_open = |_: &str| false;
        assert!(plan_hotplug(
            &prefs,
            none_open,
            &HashSet::new(),
            std::slice::from_ref(&laptop),
            Some(0)
        )
        .is_empty());
        assert_eq!(
            plan_hotplug(&prefs, none_open, &HashSet::new(), &[laptop, proj], Some(0)),
            vec![HotplugAction::Place {
                label: "stage".into(),
                monitor: 1
            }]
        );
    }

    #[test]
    fn closed_windows_and_windowed_previews_are_left_alone() {
        let laptop = mon(0, "eDP-1", 0, 1920, 1080);
        let mut prefs = prefs_with("output-main", &laptop);
        prefs.windows.get_mut("output-main").unwrap().open = false;
        assert!(plan_hotplug(&prefs, |_| false, &HashSet::new(), &[], None).is_empty());
        // A windowed preview has no remembered monitor: never parked.
        let mut windowed = Prefs::default();
        windowed.windows.insert(
            "output-main".into(),
            WinPref {
                open: true,
                monitor: None,
            },
        );
        assert!(plan_hotplug(&windowed, |_| true, &HashSet::new(), &[laptop], Some(0)).is_empty());
    }

    #[test]
    fn operator_window_cannot_be_closed_in_service_mode() {
        assert!(may_close_operator(false));
        assert!(!may_close_operator(true));
    }

    #[test]
    fn prefs_survive_a_round_trip_and_ignore_garbage() {
        let m = mon(1, "HDMI-1", 1920, 1920, 1080);
        let p = prefs_with("stage", &m);
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(Prefs::parse(&json), p);
        assert_eq!(Prefs::parse("not json"), Prefs::default());
    }
}
