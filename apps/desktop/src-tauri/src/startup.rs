//! Start-up failures. The release binary has no console (`windows_subsystem = "windows"`), so a
//! failure to open the library must be shown in a native dialog and written to the log, never
//! just end the process silently.

use crate::state::{now_ms, recover_unusable, AppState, OpenError, Paths};

/// What the user chose in the failure dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Set the unusable library aside and start from the newest backup (or an empty library).
    StartFromBackup,
    /// Open the data folder in the file manager and quit.
    OpenFolder,
    Quit,
}

/// Opens the application state. A failure is logged and put to `ask`; when the library itself is
/// the problem and the user agrees, it is replaced by a backup / an empty library and opening is
/// retried once. The unusable file is always kept next to the new one.
pub fn open_with_recovery(
    paths: &Paths,
    ask: &dyn Fn(&OpenError, &Paths) -> Choice,
    open_folder: &dyn Fn(&Paths),
) -> Result<AppState, String> {
    let err = match AppState::open(paths.clone()) {
        Ok(state) => return Ok(state),
        Err(e) => e,
    };
    log::error!("could not start HollyShow: {err}");
    match (ask(&err, paths), &err) {
        (Choice::StartFromBackup, OpenError::Database(_)) => {
            let (_store, how) = recover_unusable(paths, now_ms())?;
            log::warn!("library replaced after a start-up failure: {how:?}");
            AppState::open(paths.clone()).map_err(|e| e.to_string())
        }
        (Choice::OpenFolder, _) => {
            open_folder(paths);
            Err(err.to_string())
        }
        _ => Err(err.to_string()),
    }
}

/// Text of the failure dialog (Spanish, the app's default language).
pub fn failure_text(err: &OpenError, paths: &Paths) -> String {
    let mut t = format!(
        "HollyShow no pudo iniciar.\n\n{err}\n\nCarpeta de datos:\n{}",
        paths.data_dir.display()
    );
    if matches!(err, OpenError::Database(_)) {
        t.push_str(
            "\n\nPuedes empezar desde la copia de seguridad más reciente (o con una biblioteca \
             vacía si no hay copias). La base actual no se borra: queda junto a la nueva.",
        );
    }
    t
}

/// Native dialog; works without a webview or a console.
pub fn ask_user(err: &OpenError, paths: &Paths) -> Choice {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let text = failure_text(err, paths);
    let can_recover = matches!(err, OpenError::Database(_));
    let buttons = if can_recover {
        MessageButtons::YesNoCancelCustom(
            "Usar copia de seguridad".into(),
            "Abrir carpeta de datos".into(),
            "Salir".into(),
        )
    } else {
        MessageButtons::OkCancelCustom("Abrir carpeta de datos".into(), "Salir".into())
    };
    let res = MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("HollyShow")
        .set_description(text)
        .set_buttons(buttons)
        .show();
    match res {
        MessageDialogResult::Yes => Choice::StartFromBackup,
        MessageDialogResult::No => Choice::OpenFolder,
        MessageDialogResult::Custom(c) if c == "Usar copia de seguridad" => Choice::StartFromBackup,
        MessageDialogResult::Custom(c) if c == "Abrir carpeta de datos" => Choice::OpenFolder,
        _ => Choice::Quit,
    }
}

/// Shows an error that has no recovery (the webview could not be created...).
pub fn show_fatal(message: &str) {
    use rfd::{MessageButtons, MessageDialog, MessageLevel};
    let _ = MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("HollyShow")
        .set_description(format!("HollyShow no pudo iniciar.\n\n{message}"))
        .set_buttons(MessageButtons::Ok)
        .show();
}

/// Opens `paths.data_dir` in the platform file manager.
pub fn open_data_folder(paths: &Paths) {
    let dir = &paths.data_dir;
    let _ = std::fs::create_dir_all(dir);
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(program).arg(dir).spawn() {
        log::warn!("could not open {}: {e}", dir.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn paths(dir: &std::path::Path) -> Paths {
        Paths::new(dir.to_path_buf(), vec![])
    }

    /// A database whose schema is newer than this build.
    fn newer_database(p: &Paths) {
        let c = rusqlite::Connection::open(&p.db).unwrap();
        c.pragma_update(None, "user_version", 9999).unwrap();
    }

    #[test]
    fn a_database_from_a_newer_version_can_be_replaced_on_request() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        newer_database(&p);
        let asked = Cell::new(0);
        let r = open_with_recovery(
            &p,
            &|e, _| {
                asked.set(asked.get() + 1);
                assert!(matches!(e, OpenError::Database(_)), "{e:?}");
                Choice::StartFromBackup
            },
            &|_| {},
        );
        assert!(r.is_ok(), "{:?}", r.err());
        assert_eq!(asked.get(), 1);
        assert!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains(".corrupt-")),
            "the unusable database is kept"
        );
    }

    #[test]
    fn declining_quits_with_the_error_and_leaves_the_database_alone() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        newer_database(&p);
        let opened = Cell::new(false);
        let r = open_with_recovery(&p, &|_, _| Choice::OpenFolder, &|_| opened.set(true));
        assert!(r.is_err());
        assert!(opened.get());
        assert!(p.db.exists());
    }

    #[test]
    fn dialog_text_names_the_problem_and_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let t = failure_text(&OpenError::Database("version 9999".into()), &p);
        assert!(t.contains("version 9999") && t.contains(&dir.path().display().to_string()));
        assert!(t.contains("copia de seguridad"));
        assert!(!failure_text(&OpenError::Other("x".into()), &p).contains("copia de seguridad"));
    }
}
