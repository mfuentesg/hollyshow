//! Show-control IPC commands invoked by the webviews. Thin on purpose: authorisation,
//! enrichment and queries live in `hollyshow-app`, shared with the LAN server. The webviews are
//! trusted, so they act as the local operator.
//!
//! These commands are `async` so they run on the async runtime and never on the main thread
//! (which pumps window events for the projector windows): a slow disk must not freeze a window.

use crate::state::AppState;
use hollyshow_app::ClientCtx;
use hollyshow_core::{
    Ack, Command, CommandId, PresentationState, QueryRequest, QueryResponse, WireError,
};
use tauri::State;

#[tauri::command(async)]
pub fn dispatch(state: State<'_, AppState>, id: Option<String>, cmd: Command) -> Ack {
    let id = id.map(CommandId::new);
    state
        .app
        .dispatch(&ClientCtx::local_operator(), id.as_ref(), cmd)
}

#[tauri::command(async)]
pub fn query(state: State<'_, AppState>, query: QueryRequest) -> Result<QueryResponse, WireError> {
    state.app.query(&ClientCtx::local_operator(), &query)
}

#[tauri::command(async)]
pub fn get_snapshot(state: State<'_, AppState>) -> PresentationState {
    state.app.engine().snapshot()
}
