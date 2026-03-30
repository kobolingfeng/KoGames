#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod games;
mod game_library;
mod paths;
mod power;
mod types;

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // Only allow http/https URLs to prevent command injection
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Invalid URL scheme".to_string());
    }
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            games::get_games,
            games::save_games,
            games::launch_game,
            games::delete_game,
            games::select_game_exe,
            games::import_steam_games,
            games::get_steam_video_url,
            games::download_steam_covers,
            games::download_game_cover,
            games::fetch_steam_metadata,
            games::update_game,
            games::select_cover_image,
            games::open_folder,
            games::find_save_paths,
            game_library::import_platform_games,
            game_library::import_all_platform_games,
            game_library::detect_installed_platforms,
            power::get_battery_status,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
