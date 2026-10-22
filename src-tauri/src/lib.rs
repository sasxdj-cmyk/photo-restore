#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod ai;
mod commands;
mod state;

/// Punto di ingresso dell'applicazione desktop.
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::open_dialog,
            commands::load_paths,
            commands::preview_images,
            commands::detect_photos,
            commands::apply_crop,
            commands::apply_perspective,
            commands::apply_rotate,
            commands::apply_resize,
            commands::apply_adjust,
            commands::apply_jpeg_restore,
            commands::remove_background,
            commands::auto_remove_background,
            commands::sfondo_ai,
            commands::undo,
            commands::redo,
            commands::reset_to_original,
            commands::get_session_state,
            commands::export_image,
            commands::ai_models,
            commands::ai_upscale,
            commands::volto_ai,
            commands::dona,
            commands::imposta_fusione
        ])
        .run(tauri::generate_context!())
        .expect("errore avviando l'applicazione Photo Restore");
}
