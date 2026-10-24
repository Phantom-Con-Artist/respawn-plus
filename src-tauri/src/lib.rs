// SPDX-License-Identifier: MIT
mod database;
mod discovery;
mod display;
pub mod library;
mod recovery;
pub mod rooms;
mod runtime;

use serde::Serialize;
use std::sync::Arc;
use tauri::Manager;

#[derive(Serialize)]
pub struct EnvironmentStatus {
    database_ready: bool,
    database_path: String,
    runtimes: Vec<runtime::Runtime>,
}

#[tauri::command]
fn get_environment_status() -> Result<EnvironmentStatus, String> {
    environment_status()
}

#[tauri::command]
async fn list_games(
    service: tauri::State<'_, Arc<library::Launcher>>,
) -> Result<Vec<library::Game>, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.games())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn scan_games(
    service: tauri::State<'_, Arc<library::Launcher>>,
    directory: Option<String>,
) -> Result<library::ScanReport, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.scan(directory))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_game(
    service: tauri::State<'_, Arc<library::Launcher>>,
    path: String,
    name: Option<String>,
) -> Result<library::Game, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.import(std::path::Path::new(&path), name.as_deref(), "manual")
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_game(
    service: tauri::State<'_, Arc<library::Launcher>>,
    settings: library::GameSettings,
) -> Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.save(settings))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn launch_game(
    service: tauri::State<'_, Arc<library::Launcher>>,
    game_id: i64,
) -> Result<library::Launch, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.launch(game_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn game_log(
    service: tauri::State<'_, Arc<library::Launcher>>,
    game_id: i64,
) -> Result<String, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.log(game_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn create_room_checkpoint(
    service: tauri::State<'_, Arc<library::Launcher>>,
    game_id: i64,
) -> Result<i64, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.create_checkpoint(game_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn configure_room(
    service: tauri::State<'_, Arc<library::Launcher>>,
    game_id: i64,
    profile: rooms::RoomProfile,
) -> Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.configure_room(game_id, profile))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_room(
    service: tauri::State<'_, Arc<library::Launcher>>,
    game_id: i64,
) -> Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.stop_room(game_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn install_windows_runtime(
    service: tauri::State<'_, Arc<library::Launcher>>,
    source: String,
) -> Result<serde_json::Value, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.install_windows_runtime(source))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_windows_runtimes(
    service: tauri::State<'_, Arc<library::Launcher>>,
) -> Result<serde_json::Value, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.windows_runtimes())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn restore_room_checkpoint(
    service: tauri::State<'_, Arc<library::Launcher>>,
    game_id: i64,
    checkpoint_id: i64,
) -> Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.restore_checkpoint(game_id, checkpoint_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_tool(
    service: tauri::State<'_, Arc<library::Launcher>>,
    settings: library::ToolSettings,
) -> Result<library::Tool, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.save_tool(settings))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn launch_tool(
    service: tauri::State<'_, Arc<library::Launcher>>,
    tool_id: i64,
) -> Result<library::Launch, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.launch_tool(tool_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn remove_tool(
    service: tauri::State<'_, Arc<library::Launcher>>,
    tool_id: i64,
) -> Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.remove_tool(tool_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tool_log(
    service: tauri::State<'_, Arc<library::Launcher>>,
    tool_id: i64,
) -> Result<String, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.tool_log(tool_id))
        .await
        .map_err(|e| e.to_string())?
}

pub fn environment_status() -> Result<EnvironmentStatus, String> {
    let base = dirs::data_dir()
        .ok_or("Could not locate the platform data directory")?
        .join("respawn-plus");
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let path = base.join("respawn-plus.sqlite3");
    database::open(&path).map_err(|e| e.to_string())?;
    Ok(EnvironmentStatus {
        database_ready: true,
        database_path: path.display().to_string(),
        runtimes: runtime::discover(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(
                library::Launcher::new().map_err(std::io::Error::other)?,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_environment_status,
            list_games,
            scan_games,
            import_game,
            save_game,
            launch_game,
            game_log,
            create_room_checkpoint,
            restore_room_checkpoint,
            configure_room,
            stop_room,
            install_windows_runtime,
            list_windows_runtimes,
            save_tool,
            launch_tool,
            remove_tool,
            tool_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running Respawn+");
}
