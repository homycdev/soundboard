use std::sync::Arc;

use tauri::State;

use crate::coordinator::Coordinator;
use crate::dto::{
    AppSnapshot, AudioRoutingInput, AudioRoutingSnapshot, CommandTrigger, PlayResult, ShortcutInput,
};
use crate::error::ApiError;

pub struct AppState {
    pub coordinator: Arc<Coordinator>,
}

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> Result<AppSnapshot, ApiError> {
    state.coordinator.get_state()
}

#[tauri::command]
pub async fn get_audio_routing(
    state: State<'_, AppState>,
) -> Result<AudioRoutingSnapshot, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.get_audio_routing())
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn configure_audio_routing(
    state: State<'_, AppState>,
    input: AudioRoutingInput,
) -> Result<AudioRoutingSnapshot, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.configure_audio_routing(input))
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command]
pub async fn disable_audio_routing(
    state: State<'_, AppState>,
) -> Result<AudioRoutingSnapshot, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.disable_audio_routing())
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_shortcut_capture_active(
    state: State<'_, AppState>,
    active: bool,
) -> Result<(), ApiError> {
    state.coordinator.set_shortcut_capture_active(active)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn pick_and_import_sound(
    state: State<'_, AppState>,
    cell_id: String,
) -> Result<Option<AppSnapshot>, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.pick_and_import_sound(cell_id))
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn pick_and_replace_sound(
    state: State<'_, AppState>,
    cell_id: String,
) -> Result<Option<AppSnapshot>, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.pick_and_replace_sound(cell_id))
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command(rename_all = "camelCase")]
pub fn play_sound(
    state: State<'_, AppState>,
    cell_id: String,
    trigger: CommandTrigger,
) -> Result<PlayResult, ApiError> {
    state.coordinator.play_sound(cell_id, trigger.into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn delete_sound(
    state: State<'_, AppState>,
    cell_id: String,
) -> Result<AppSnapshot, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.delete_sound(cell_id))
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_shortcut(
    state: State<'_, AppState>,
    cell_id: String,
    shortcut: ShortcutInput,
) -> Result<AppSnapshot, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.set_shortcut(cell_id, shortcut))
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn clear_shortcut(
    state: State<'_, AppState>,
    cell_id: String,
) -> Result<AppSnapshot, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.clear_shortcut(cell_id))
        .await
        .map_err(|_| ApiError::internal())?
}

#[tauri::command]
pub async fn resize_grid(
    state: State<'_, AppState>,
    rows: u8,
    columns: u8,
) -> Result<AppSnapshot, ApiError> {
    let coordinator = Arc::clone(&state.coordinator);
    tauri::async_runtime::spawn_blocking(move || coordinator.resize_grid(rows, columns))
        .await
        .map_err(|_| ApiError::internal())?
}
