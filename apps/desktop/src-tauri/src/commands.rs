//! IPC commands invoked by the webviews. Thin on purpose: all rules live in `hollyshow-core`
//! (reducer) and `hollyshow_storage::host` (command enrichment + queries), shared with the server.

use crate::state::AppState;
use hollyshow_core::{
    Ack, Command, CommandId, PresentationState, QueryRequest, QueryResponse, WireError,
};
use hollyshow_storage::host;
use tauri::State;

#[tauri::command]
pub fn dispatch(state: State<'_, AppState>, id: Option<String>, cmd: Command) -> Ack {
    let id = id.map(CommandId::new);
    host::dispatch(&state.engine, &state.repos, id.as_ref(), cmd)
}

#[tauri::command]
pub fn query(state: State<'_, AppState>, query: QueryRequest) -> Result<QueryResponse, WireError> {
    host::execute_query(&state.repos, &query).map_err(|e| WireError {
        code: "storage".into(),
        message: e.to_string(),
    })
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> PresentationState {
    state.engine.snapshot()
}
