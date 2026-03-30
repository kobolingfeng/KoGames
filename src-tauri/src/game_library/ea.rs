//! EA App (Origin) 游戏库扫�?

use crate::games::find_game_exe;
use crate::types::Game;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub fn is_ea_installed() -> bool {
    let paths = [
        r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EADesktop.exe",
        r"C:\Program Files (x86)\Origin\Origin.exe",
        r"C:\Program Files\Origin\Origin.exe",
    ];
    paths.iter().any(|p| std::path::Path::new(p).exists())
}

fn get_ea_games_from_registry() -> Vec<(String, String, Option<String>)> {
    let mut games = vec![];
    let ps_script = r#"
$paths = @(
    'HKLM:\SOFTWARE\EA Games',
    'HKLM:\SOFTWARE\WOW6432Node\EA Games',
    'HKLM:\SOFTWARE\Electronic Arts',
    'HKLM:\SOFTWARE\WOW6432Node\Electronic Arts'
)
foreach ($basePath in $paths) {
    if (Test-Path $basePath) {
        Get-ChildItem $basePath -ErrorAction SilentlyContinue | ForEach-Object {
            $installDir = (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).'Install Dir'
            if (-not $installDir) { $installDir = (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).InstallDir }
            if ($installDir -and (Test-Path $installDir)) { Write-Output "$($_.PSChildName)|$installDir" }
        }
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
                let name = parts[0].trim().to_string();
                let install_dir = parts[1].trim().to_string();
                let exe_path = find_game_exe(&PathBuf::from(&install_dir));
                if !name.is_empty() {
                    games.push((name, install_dir, exe_path));
                }
            }
        }
    }
    games
}

fn scan_ea_install_dirs() -> Vec<(String, String, Option<String>)> {
    let mut games = vec![];
    let install_dirs = [
        r"C:\Program Files\EA Games",
        r"C:\Program Files (x86)\EA Games",
        r"C:\Program Files (x86)\Origin Games",
        r"C:\Program Files\Origin Games",
    ];

    for dir in &install_dirs {
        let path = PathBuf::from(dir);
        if !path.exists() { continue; }
        if let Ok(entries) = std::fs::read_dir(&path) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                let game_dir = entry.path();
                let game_name = entry.file_name().to_string_lossy().to_string();
                if game_name.starts_with('.') || game_name.to_lowercase().contains("redist") { continue; }
                let exe_path = find_game_exe(&game_dir);
                games.push((game_name, game_dir.to_string_lossy().to_string(), exe_path));
            }
        }
    }
    games
}

pub fn scan_ea_games() -> Result<Vec<Game>, String> {
    let mut games = vec![];
    let mut seen = std::collections::HashSet::new();

    for (name, install_dir, exe_path) in get_ea_games_from_registry() {
        if seen.contains(&name.to_lowercase()) { continue; }
        seen.insert(name.to_lowercase());
        games.push(Game {
            id: format!("ea_{}", name.replace(' ', "_").to_lowercase()),
            name,
            path: exe_path,
            steam_app_id: None,
            source: Some("ea".to_string()),
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

    for (name, install_dir, exe_path) in scan_ea_install_dirs() {
        if seen.contains(&name.to_lowercase()) { continue; }
        seen.insert(name.to_lowercase());
        games.push(Game {
            id: format!("ea_{}", name.replace(' ', "_").to_lowercase()),
            name,
            path: exe_path,
            steam_app_id: None,
            source: Some("ea".to_string()),
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
