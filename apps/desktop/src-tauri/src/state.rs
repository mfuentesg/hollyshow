//! Application state shared by every Tauri command and the event bridge.

use hollyshow_core::{Command, Engine};
use hollyshow_storage::{memory::MemoryStore, Repos};
use std::sync::Arc;

pub struct AppState {
    pub engine: Arc<Engine>,
    pub repos: Repos,
}

impl AppState {
    pub fn new() -> Self {
        // Storage builder: replace `MemoryStore` with the SQLite repos opened under
        // `app.path().app_data_dir()`; nothing else in this crate has to change.
        let state = Self {
            engine: Arc::new(Engine::default()),
            repos: MemoryStore::repos(),
        };
        #[cfg(debug_assertions)]
        state.load_demo_show();
        state
    }

    /// Debug builds start with a few invented slides so a fresh checkout shows something.
    #[cfg(debug_assertions)]
    fn load_demo_show(&self) {
        use hollyshow_core::{ShowEntry, Slide};
        let entry = |id: &str, title: &str, slides: Vec<Slide>| ShowEntry {
            item_id: id.into(),
            title: title.into(),
            slides,
            duration_secs: None,
        };
        let entries = vec![
            entry(
                "demo-song",
                "Cancion de demostracion",
                vec![
                    Slide::lyrics(
                        "demo-song:0",
                        Some("Verso 1"),
                        &["Primera linea de ejemplo", "Segunda linea de ejemplo"],
                    ),
                    Slide::lyrics(
                        "demo-song:1",
                        Some("Coro"),
                        &["Coro de ejemplo", "Se repite en el culto"],
                    ),
                ],
            ),
            entry(
                "demo-text",
                "Bienvenida",
                vec![Slide::lyrics("demo-text:0", None, &["Bienvenidos"])],
            ),
        ];
        let _ = self.engine.dispatch(None, Command::LoadShow { entries });
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
