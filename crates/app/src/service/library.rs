//! Library writes behind the use-case layer.
//!
//! Every mutation of the library (songs, services, themes, Bibles, media...) goes through
//! [`AppService::write_library`], which (a) refuses while "modo culto" is on, (b) runs the write
//! and (c) tells the app what changed so the show is rebuilt and every client refreshes. The
//! check and the write happen under the same gate that serialises the `ServiceMode` command, so
//! a write can never slip in after the lock was taken. A transport (Tauri shell, LAN API) cannot
//! forget either step.

use super::{storage_to_core, AppService};
use hollyshow_core::{
    CoreError, LibraryScope, Playlist, PlaylistId, Song, SongId, Theme, ThemeId, WireError,
};
use hollyshow_storage_api::StorageError;
use std::sync::MutexGuard;

fn wire(code: &str, message: &str) -> WireError {
    WireError {
        code: code.into(),
        message: message.into(),
    }
}

fn storage_wire(e: &StorageError) -> WireError {
    WireError::from(&storage_to_core(e))
}

impl AppService {
    /// Serialises library writes against the `ServiceMode` command.
    pub(super) fn gate(&self) -> MutexGuard<'_, ()> {
        self.write_gate.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Whether library edits are blocked ("modo culto").
    pub fn is_locked(&self) -> bool {
        self.engine.view().service_mode
    }

    /// Cheap early check for operations that do their heavy lifting before writing (downloads,
    /// parsing). The authoritative check is the one inside [`Self::write_library`].
    pub fn ensure_unlocked(&self) -> Result<(), WireError> {
        if self.is_locked() {
            Err(WireError::from(&CoreError::Locked))
        } else {
            Ok(())
        }
    }

    /// Runs `write` if service mode is off, then publishes the change. `write` may also decline
    /// to change anything by returning `Ok`; use [`Self::write_library_if`] to skip the
    /// notification then.
    pub fn write_library<T>(
        &self,
        scope: LibraryScope,
        write: impl FnOnce() -> Result<T, WireError>,
    ) -> Result<T, WireError> {
        self.write_library_if(scope, |_| true, write)
    }

    /// Like [`Self::write_library`] but only notifies when `changed(&result)` is true.
    pub fn write_library_if<T>(
        &self,
        scope: LibraryScope,
        changed: impl FnOnce(&T) -> bool,
        write: impl FnOnce() -> Result<T, WireError>,
    ) -> Result<T, WireError> {
        let out = {
            let _gate = self.gate();
            self.ensure_unlocked()?;
            write()?
        };
        if changed(&out) {
            self.library_changed(scope);
        }
        Ok(out)
    }

    pub fn save_song(&self, mut song: Song, now_ms: i64) -> Result<Song, WireError> {
        if song.title.trim().is_empty() {
            return Err(wire("invalid", "la cancion necesita un titulo"));
        }
        if song.created_at == 0 {
            song.created_at = now_ms;
        }
        song.updated_at = now_ms;
        self.write_library(LibraryScope::Songs, || {
            self.repos.songs.save(&song).map_err(|e| storage_wire(&e))?;
            Ok(song.clone())
        })
    }

    pub fn delete_song(&self, id: &SongId) -> Result<(), WireError> {
        self.write_library(LibraryScope::Songs, || {
            self.repos.songs.delete(id).map_err(|e| storage_wire(&e))
        })
    }

    pub fn save_playlist(&self, playlist: Playlist) -> Result<Playlist, WireError> {
        {
            let _gate = self.gate();
            self.ensure_unlocked()?;
            self.repos
                .playlists
                .save(&playlist)
                .map_err(|e| storage_wire(&e))?;
        }
        self.playlist_saved(&playlist.id);
        Ok(playlist)
    }

    pub fn delete_playlist(&self, id: &PlaylistId) -> Result<(), WireError> {
        {
            let _gate = self.gate();
            self.ensure_unlocked()?;
            if self.active_playlist().as_ref() == Some(id) {
                return Err(wire("conflict", "no se puede borrar el servicio abierto"));
            }
            self.repos
                .playlists
                .delete(id)
                .map_err(|e| storage_wire(&e))?;
        }
        self.playlist_saved(id);
        Ok(())
    }

    pub fn save_theme(&self, theme: Theme) -> Result<Theme, WireError> {
        if theme.id.as_str().is_empty() {
            return Err(wire("invalid", "el tema necesita un id"));
        }
        self.write_library(LibraryScope::Themes, || {
            self.repos
                .themes
                .save(&theme)
                .map_err(|e| storage_wire(&e))?;
            Ok(theme.clone())
        })
    }

    pub fn delete_theme(&self, id: &ThemeId) -> Result<(), WireError> {
        if id.as_str() == "default" {
            return Err(wire(
                "conflict",
                "el tema predeterminado no se puede borrar",
            ));
        }
        self.write_library(LibraryScope::Themes, || {
            self.repos.themes.delete(id).map_err(|e| storage_wire(&e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture;
    use super::*;
    use hollyshow_core::{Command, Role, SessionId};

    fn lock(f: &super::super::tests::Fixture, on: bool) {
        let ack = f.app.dispatch(
            &super::super::ClientCtx::local_operator(),
            None,
            Command::ServiceMode { on },
        );
        assert!(ack.ok);
    }

    #[test]
    fn writes_are_refused_in_service_mode_and_resume_after() {
        let f = fixture();
        let theme = Theme::default_theme();
        assert!(f.app.save_theme(theme.clone()).is_ok());
        lock(&f, true);
        assert_eq!(f.app.save_theme(theme.clone()).unwrap_err().code, "locked");
        let ran = std::cell::Cell::new(false);
        let e = f
            .app
            .write_library(LibraryScope::Media, || {
                ran.set(true);
                Ok(())
            })
            .unwrap_err();
        assert_eq!(e.code, "locked");
        assert!(!ran.get(), "the write must not run while locked");
        lock(&f, false);
        assert!(f.app.save_theme(theme).is_ok());
    }

    #[test]
    fn a_remote_cannot_swap_or_refresh_the_show_while_locked() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        lock(&f, true);
        let phone = super::super::ClientCtx::new(SessionId::new("phone"), Role::Operator);
        let ack = f
            .app
            .dispatch(&phone, None, Command::SetActivePlaylist { id: "p1".into() });
        assert_eq!(ack.error.unwrap().code, "locked");
        let ack = f.app.dispatch(&phone, None, Command::RefreshShow);
        assert_eq!(ack.error.unwrap().code, "locked");
    }

    #[test]
    fn the_open_service_cannot_be_deleted() {
        let f = fixture();
        f.app.set_active_playlist(&"p1".into()).unwrap();
        assert_eq!(
            f.app.delete_playlist(&"p1".into()).unwrap_err().code,
            "conflict"
        );
    }
}
