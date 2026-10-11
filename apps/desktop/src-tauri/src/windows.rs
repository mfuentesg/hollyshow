//! Output / stage window management (PLAN §4 "Multi-monitor"). Windows are created at runtime so
//! the operator can pick the screen; they are borderless and never use native macOS fullscreen
//! (separate Space problems), only a monitor-sized always-on-top window.

use hollyshow_core::{MonitorInfo, WireError};
use tauri::{AppHandle, Manager, Monitor, WebviewUrl, WebviewWindowBuilder};

fn err(code: &str, message: impl ToString) -> WireError {
    WireError {
        code: code.into(),
        message: message.to_string(),
    }
}

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

fn open(
    app: &AppHandle,
    label: &str,
    route: &str,
    title: &str,
    monitor: Option<usize>,
) -> Result<(), WireError> {
    if let Some(w) = app.get_webview_window(label) {
        w.show().map_err(|e| err("window", e))?;
        return Ok(());
    }
    let mut b = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("index.html#{route}").into()),
    )
    .title(title)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false);

    if let Some(i) = monitor {
        let monitors = app.available_monitors().map_err(|e| err("monitor", e))?;
        let m = monitors
            .get(i)
            .ok_or_else(|| err("monitor", format!("no monitor {i}")))?;
        let p = m.position();
        let s = m.size();
        let (x, y, w, h) = logical_rect((p.x, p.y), (s.width, s.height), m.scale_factor());
        b = b.position(x, y).inner_size(w, h);
    } else {
        b = b.inner_size(960.0, 540.0);
    }
    b.build().map(|_| ()).map_err(|e| err("window", e))
}

#[tauri::command]
pub fn open_output(app: AppHandle, id: String, monitor: Option<usize>) -> Result<(), WireError> {
    let label = output_label(&id)?;
    open(
        &app,
        &label,
        &format!("/output/{id}"),
        "HollyShow - Salida",
        monitor,
    )
}

#[tauri::command]
pub fn open_stage(app: AppHandle, monitor: Option<usize>) -> Result<(), WireError> {
    open(&app, "stage", "/stage", "HollyShow - Escenario", monitor)
}

#[tauri::command]
pub fn close_window(app: AppHandle, label: String) -> Result<(), WireError> {
    // The operator window can never be closed through this command.
    if label == "main" {
        return Err(err("forbidden", "cannot close the operator window"));
    }
    match app.get_webview_window(&label) {
        Some(w) => w.close().map_err(|e| err("window", e)),
        None => Ok(()),
    }
}

fn info(index: usize, m: &Monitor, primary: Option<&Monitor>) -> MonitorInfo {
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

#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, WireError> {
    let primary = app.primary_monitor().map_err(|e| err("monitor", e))?;
    let all = app.available_monitors().map_err(|e| err("monitor", e))?;
    Ok(all
        .iter()
        .enumerate()
        .map(|(i, m)| info(i, m, primary.as_ref()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_restricted() {
        assert_eq!(output_label("main").unwrap(), "output-main");
        assert_eq!(output_label("lobby-2").unwrap(), "output-lobby-2");
        for bad in ["", "Main", "a b", "../x", "a/b", &"x".repeat(40)] {
            assert!(output_label(bad).is_err(), "{bad}");
        }
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
}
