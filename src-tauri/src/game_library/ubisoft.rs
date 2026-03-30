//! Ubisoft Connect (Uplay) 游戏库扫�?

use crate::games::find_game_exe;
use crate::types::Game;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub fn is_ubisoft_installed() -> bool {
    let paths = [
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\UbisoftConnect.exe",
        r"C:\Program Files\Ubisoft\Ubisoft Game Launcher\UbisoftConnect.exe",
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\Uplay.exe",
    ];
    paths.iter().any(|p| std::path::Path::new(p).exists())
}

fn get_ubisoft_games_from_registry() -> Vec<(String, String, Option<String>)> {
    let mut games = vec![];
    let ps_script = r#"
$basePath = 'HKLM:\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs'
if (-not (Test-Path $basePath)) { $basePath = 'HKLM:\SOFTWARE\Ubisoft\Launcher\Installs' }
if (Test-Path $basePath) {
    Get-ChildItem $basePath -ErrorAction SilentlyContinue | ForEach-Object {
        $appId = $_.PSChildName
        $installDir = (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).InstallDir
        if ($installDir -and (Test-Path $installDir)) { Write-Output "$appId|$installDir" }
    }
}
"#;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps_script]);

    #[cfg(windows)]
    let output = output.creation_flags(0x08000000);

    let output = output.output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 2 {
                let app_id = parts[0].trim().to_string();
                let install_dir = parts[1].trim().to_string();
                let game_name = PathBuf::from(&install_dir)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("Ubisoft Game {}", app_id));
                let exe_path = find_game_exe(&PathBuf::from(&install_dir));
                games.push((game_name, install_dir, exe_path));
            }
        }
    }
    games
}

fn scan_ubisoft_install_dirs() -> Vec<(String, String, Option<String>)> {
    let mut games = vec![];
    let install_dirs = [
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games",
        r"C:\Program Files\Ubisoft\Ubisoft Game Launcher\games",
        r"D:\Ubisoft\Ubisoft Game Launcher\games",
        r"E:\Ubisoft\Ubisoft Game Launcher\games",
    ];

    for dir in &install_dirs {
        let path = PathBuf::from(dir);
        if !path.exists() { continue; }
        if let Ok(entries) = std::fs::read_dir(&path) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                let game_dir = entry.path();
                let game_name = entry.file_name().to_string_lossy().to_string();
                if game_name.starts_with('.') { continue; }
                let exe_path = find_game_exe(&game_dir);
                games.push((game_name, game_dir.to_string_lossy().to_string(), exe_path));
            }
        }
    }
    games
}

pub fn scan_ubisoft_games() -> Result<Vec<Game>, String> {
    let mut games = vec![];
    let mut seen = std::collections::HashSet::new();

    for (name, install_dir, exe_path) in get_ubisoft_games_from_registry() {
        if seen.contains(&name.to_lowercase()) { continue; }
        seen.insert(name.to_lowercase());
        games.push(Game {
            id: format!("ubisoft_{}", name.replace(' ', "_").to_lowercase()),
            name,
            path: exe_path,
            steam_app_id: None,
            source: Some("ubisoft".to_string()),
            cover: None,
            added_at: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64),
            last_played_at: None,
            install_location: Some(install_dir),
            pinned: None,
            completion_status: None,
            total_play_time: None,
            description: None,
            genre: None,
            release_year: None,
            favorite: None,
        });
    }

    for (name, install_dir, exe_path) in scan_ubisoft_install_dirs() {
        if seen.contains(&name.to_lowercase()) { continue; }
        seen.insert(name.to_lowercase());
        games.push(Game {
            id: format!("ubisoft_{}", name.replace(' ', "_").to_lowercase()),
            name,
            path: exe_path,
            steam_app_id: None,
            source: Some("ubisoft".to_string()),
            cover: None,
            added_at: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64),
            last_played_at: None,
            install_location: Some(install_dir),
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
