mod audio;
mod commands;
mod coordinator;
mod domain;
mod dto;
mod error;
mod events;
mod hotkeys;
mod import;
mod persistence;
mod ports;

use std::sync::Arc;

use audio::KiraAudioService;
use commands::AppState;
use coordinator::Coordinator;
use events::TauriEventSink;
use hotkeys::tauri_service::{ShortcutRouter, TauriHotkeyService};
use import::tauri_picker::TauriFilePicker;
use persistence::JsonRepository;
use ports::{AudioService, FilePicker, PlaybackEventSink, StateRepository};
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();

    let router = Arc::new(ShortcutRouter::default());
    let shortcut_router = Arc::clone(&router);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |_app, shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        shortcut_router.handle_pressed(shortcut.id());
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let app_data_dir = app.path().app_data_dir()?;
            let repository = Arc::new(JsonRepository::new(app_data_dir)?);
            let repository_load = repository.load();
            let event_sink: Arc<dyn PlaybackEventSink> =
                Arc::new(TauriEventSink::new(app.handle().clone()));
            let concrete_audio = KiraAudioService::start(event_sink);
            let audio: Arc<dyn AudioService> = concrete_audio;
            router.set_audio(Arc::clone(&audio));
            let hotkeys = Arc::new(TauriHotkeyService::new(
                app.handle().clone(),
                Arc::clone(&router),
            ));
            let picker = Arc::new(TauriFilePicker::new(app.handle().clone()));
            let repository_trait: Arc<dyn StateRepository> = repository;
            let hotkeys_trait: Arc<dyn hotkeys::HotkeyService> = hotkeys;
            let picker_trait: Arc<dyn FilePicker> = picker;

            let coordinator = match repository_load {
                Ok(load) => Coordinator::initialize(
                    load,
                    Arc::clone(&repository_trait),
                    Arc::clone(&audio),
                    Arc::clone(&hotkeys_trait),
                    Arc::clone(&picker_trait),
                ),
                Err(error) => Coordinator::blocked(
                    error,
                    repository_trait,
                    audio,
                    hotkeys_trait,
                    picker_trait,
                ),
            };
            app.manage(AppState { coordinator });
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_shortcut_capture_active,
            commands::pick_and_import_sound,
            commands::pick_and_replace_sound,
            commands::play_sound,
            commands::delete_sound,
            commands::set_shortcut,
            commands::clear_shortcut,
            commands::resize_grid,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Soundboard");
}
