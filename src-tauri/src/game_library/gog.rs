//! GOG Galaxy 游戏库扫�?

use crate::games::find_game_exe;
use crate::types::Game;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub fn is_gog_installed() -> bool {
    let paths = [
        r"C:\Program Files (x86)\GOG Galaxy\GalaxyClient.exe",
        r"C:\Program Files\GOG Galaxy\GalaxyClient.exe",
    ];
    paths.iter().any(|p| std::path::Path::new(p).exists())
}

struct GogGameInfo {
    game_id: String,
    name: String,
    install_path: String,
    exe_path: Option<String>,
}

fn get_gog_games_from_registry() -> Vec<GogGameInfo> {
    let mut games = vec![];
    let ps_script = r#"
$basePaths = @('HKLM:\SOFTWARE\GOG.com\Games', 'HKLM:\SOFTWARE\WOW6432Node\GOG.com\Games')
foreach ($basePath in $basePaths) {
    if (Test-Path $basePath) {
        Get-ChildItem $basePath -ErrorAction SilentlyContinue | ForEach-Object {
            $gameId = $_.PSChildName
            $props = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
            $gameName = $props.GAMENAME; $path = $props.PATH; $exePath = $props.EXE
            if ($gameName -and $path -and (Test-Path $path)) { Write-Output "$gameId|$gameName|$path|$exePath" }
        }
    }
}
"#;

    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps_script]);

    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let output = cmd.output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 3 {
                let game_id = parts[0].trim().to_string();
                let name = parts[1].trim().to_string();
                let install_path = parts[2].trim().to_string();
                let exe_path = if parts.len() > 3 && !parts[3].trim().is_empty() {
                    let exe = parts[3].trim();
                    let full = PathBuf::from(&install_path).join(exe);
                    if full.exists() { Some(full.to_string_lossy().to_string()) }
                    else if PathBuf::from(exe).exists() { Some(exe.to_string()) }
                    else { None }
                } else { None };
                games.push(GogGameInfo { game_id, name, install_path, exe_path });
            }
        }
    }
    games
}

fn scan_gog_install_dirs() -> Vec<GogGameInfo> {
    let mut games = vec![];
    let install_dirs = [
        r"C:\GOG Games",
        r"C:\Program Files (x86)\GOG Games",
        r"C:\Program Files\GOG Games",
        r"D:\GOG Games",
        r"E:\GOG Games",
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

                let has_gog_info = std::fs::read_dir(&game_dir)
                    .map(|entries| entries.flatten().any(|e| {
                        let n = e.file_name().to_string_lossy().to_string();
                        n.starts_with("goggame-") && n.ends_with(".info")
                    }))
                    .unwrap_or(false);
                if !has_gog_info { continue; }

                let exe_path = find_game_exe(&game_dir);
                games.push(GogGameInfo {
                    game_id: format!("dir_{}", game_name.replace(' ', "_")),
                    name: game_name,
                    install_path: game_dir.to_string_lossy().to_string(),
                    exe_path,
                });
            }
        }
    }
    games
}

pub fn scan_gog_games() -> Result<Vec<Game>, String> {
    let mut games = vec![];
    let mut seen = std::collections::HashSet::new();

    for info in get_gog_games_from_registry() {
        if seen.contains(&info.game_id) { continue; }
        seen.insert(info.game_id.clone());
        games.push(Game::new_import(
            format!("gog_{}", info.game_id),
            info.name,
            "gog",
            info.exe_path,
            Some(info.install_path),
        ));
    }

    for info in scan_gog_install_dirs() {
        if seen.contains(&info.game_id) { continue; }
        seen.insert(info.game_id.clone());
        games.push(Game::new_import(
            format!("gog_{}", info.game_id),
            info.name,
            "gog",
            info.exe_path,
            Some(info.install_path),
        ));
    }

    Ok(games)
}
