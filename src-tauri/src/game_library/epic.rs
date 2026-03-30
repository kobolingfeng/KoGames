//! Epic Games Store 游戏库扫描

use crate::types::Game;
use std::path::PathBuf;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EpicManifest {
    app_name: Option<String>,
    display_name: Option<String>,
    install_location: Option<String>,
    launch_executable: Option<String>,
    #[serde(default)]
    is_dlc: bool,
}

fn get_epic_data_path() -> Option<PathBuf> {
    let program_data = std::env::var("PROGRAMDATA").ok()?;
    let manifests_dir = PathBuf::from(program_data)
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");
    if manifests_dir.exists() { Some(manifests_dir) } else { None }
}

pub fn is_epic_installed() -> bool {
    let paths = [
        r"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win32\EpicGamesLauncher.exe",
        r"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
        r"C:\Program Files\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
    ];
    paths.iter().any(|p| std::path::Path::new(p).exists()) || get_epic_data_path().is_some()
}

pub fn scan_epic_games() -> Result<Vec<Game>, String> {
    use std::collections::HashSet;

    let manifests_dir = get_epic_data_path().ok_or_else(|| "Epic Games data not found".to_string())?;
    let mut games = vec![];
    let mut seen: HashSet<String> = HashSet::new();

    let entries = std::fs::read_dir(&manifests_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let manifest: EpicManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if manifest.is_dlc { continue; }

        let app_name = match manifest.app_name { Some(n) => n, None => continue };
        if seen.contains(&app_name) { continue; }
        seen.insert(app_name.clone());

        let display_name = match manifest.display_name { Some(n) => n, None => continue };
        if display_name.contains("Proton") || display_name.contains("Redistributable") { continue; }

        let exe_path = if let (Some(loc), Some(exe)) = (&manifest.install_location, &manifest.launch_executable) {
            let full = PathBuf::from(loc).join(exe);
            if full.exists() { Some(full.to_string_lossy().to_string()) } else { None }
        } else { None };

        let install_loc = manifest.install_location.clone();

        games.push(Game {
            id: format!("epic_{}", app_name),
            name: display_name,
            path: exe_path,
            steam_app_id: None,
            source: Some("epic".to_string()),
            cover: None,
            added_at: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64),
            last_played_at: None,
            install_location: install_loc,
            pinned: None,
            completion_status: None,
            total_play_time: None,
            description: None,
            genre: None,
            release_year: None,
            favorite: None,
        });
    }

    Ok(games)
}
