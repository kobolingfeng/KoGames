#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod games;
mod game_library;
mod paths;
mod power;
mod types;

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Invalid URL scheme".to_string());
    }
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_launch_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let auto_backup = args.contains(&"--backup".to_string());

    if args.contains(&"--version".to_string()) {
        println!("KoGames v{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args.contains(&"--help".to_string()) {
        println!("KoGames - Game Library Manager");
        println!("Usage: kogames [options]");
        println!("  --version      Show version");
        println!("  --help         Show help");
        println!("  --backup       Create backup on startup");
        println!("  --minimized    Start minimized");
        return;
    }

    let builder = tauri::Builder::default()
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
            games::fetch_steam_videos,
            games::update_game,
            games::select_cover_image,
            games::open_folder,
            games::find_save_paths,
            games::backup_library,
            games::restore_backup,
            games::list_backups,

            games::get_library_stats,
            games::search_cover_online,
            games::fetch_igdb_metadata,
            games::detect_install_size,
            games::export_library,
            games::import_library,
            games::auto_import_all,
            games::batch_fetch_metadata,
            games::save_filter_presets,
            games::load_filter_presets,
            games::detect_duplicates,
            games::get_play_timeline,
            games::save_user_settings,
            games::load_user_settings,
            games::check_library_health,
            games::launch_url,
            games::get_year_in_review,
            games::get_milestones,
            games::get_recommendations,
            games::query_games,
            games::add_journal_entry,
            games::get_journal_entries,
            games::fetch_howlongtobeat,
            games::log_activity,
            games::get_recent_activity,
            games::fetch_pcgw_info,
            games::find_screenshots,
            games::detect_special_k,
            games::launch_with_special_k,
            games::save_launch_config,
            games::load_launch_config,
            get_launch_args,
            game_library::import_platform_games,
            game_library::import_all_platform_games,
            game_library::detect_installed_platforms,
            power::get_battery_status,
            open_url,
        ]);

    let builder = builder.setup(move |app| {
        use tauri::Manager;
        use tauri::menu::{MenuBuilder, MenuItemBuilder};
        use tauri::tray::TrayIconBuilder;

        let show_item = MenuItemBuilder::with_id("show", "Show KoGames").build(app)?;
        let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
        let menu = MenuBuilder::new(app)
            .item(&show_item)
            .separator()
            .item(&quit_item)
            .build()?;

        let _tray = TrayIconBuilder::new()
            .menu(&menu)
            .tooltip("KoGames")
            .on_menu_event(move |app, event| {
                match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        std::process::exit(0);
                    }
                    _ => {}
                }
            })
            .on_tray_icon_event(|tray, event| {
                if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                    let app = tray.app_handle();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
            })
            .build(app)?;

        if auto_backup {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = games::backup_library(handle).await;
                println!("[KoGames] Auto-backup completed");
            });
        }

        Ok(())
    });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
