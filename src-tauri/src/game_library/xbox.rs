//! Xbox / Microsoft Store 游戏库扫描

use crate::types::Game;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub fn is_xbox_installed() -> bool {
    #[cfg(windows)]
    {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
                "Get-AppxPackage -Name Microsoft.XboxApp -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name"
            ])
            .creation_flags(0x08000000)
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout.contains("XboxApp") || stdout.contains("Xbox");
        }
    }
    true
}

struct XboxGameInfo {
    name: String,
    package_family_name: String,
    install_location: String,
    app_id: Option<String>,
}

fn get_xbox_games_from_appx() -> Vec<XboxGameInfo> {
    let mut games = vec![];

    let ps_script = r#"
$apps = Get-AppxPackage -AllUsers | Where-Object {
    $_.IsFramework -eq $false -and
    $_.SignatureKind -ne 'System' -and
    $_.Name -notmatch '^Microsoft\.(Windows|NET|VCLibs|UI|Services|Store)' -and
    $_.Name -notmatch '(Framework|Runtime|Extension)' -and
    (
        $_.Name -match '(Game|Xbox)' -or
        $_.PublisherDisplayName -match '(Games|Gaming|Entertainment)' -or
        (Test-Path "$($_.InstallLocation)\MicrosoftGame.config")
    )
}
foreach ($app in $apps) {
    if ($app.InstallLocation -and (Test-Path $app.InstallLocation)) {
        $displayName = $app.Name
        $manifestPath = Join-Path $app.InstallLocation "AppxManifest.xml"
        if (Test-Path $manifestPath) {
            try {
                $manifest = [xml](Get-Content $manifestPath)
                $dn = $manifest.Package.Properties.DisplayName
                if ($dn -and $dn -notmatch '^ms-resource:') { $displayName = $dn }
            } catch {}
        }
        $appId = ""
        try { $manifest = [xml](Get-Content $manifestPath); $appId = $manifest.Package.Applications.Application.Id } catch {}
        Write-Output "$displayName|$($app.PackageFamilyName)|$($app.InstallLocation)|$appId"
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
            if parts.len() >= 3 {
                let name = parts[0].trim().to_string();
                let pfn = parts[1].trim().to_string();
                let loc = parts[2].trim().to_string();
                let app_id = if parts.len() > 3 && !parts[3].trim().is_empty() {
                    Some(parts[3].trim().to_string())
                } else { None };
                if name.is_empty() || name.starts_with("Microsoft.") { continue; }
                games.push(XboxGameInfo { name, package_family_name: pfn, install_location: loc, app_id });
            }
        }
    }
    games
}

fn is_game_directory(install_location: &str) -> bool {
    let path = PathBuf::from(install_location);
    if path.join("MicrosoftGame.config").exists() { return true; }
    if let Ok(entries) = std::fs::read_dir(&path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".exe") {
                if let Ok(meta) = entry.metadata() {
                    if meta.len() > 10 * 1024 * 1024 { return true; }
                }
            }
        }
    }
    false
}

pub fn scan_xbox_games() -> Result<Vec<Game>, String> {
    use std::collections::HashSet;

    let mut games = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    let appx_games = get_xbox_games_from_appx();

    for info in appx_games {
        if !is_game_directory(&info.install_location) { continue; }
        if seen.contains(&info.package_family_name) { continue; }
        seen.insert(info.package_family_name.clone());

        let app_id = info.app_id.as_deref().unwrap_or("App");
        let launch_uri = format!("shell:appsFolder\\{}!{}", info.package_family_name, app_id);

        games.push(Game {
            id: format!("xbox_{}", info.package_family_name.replace('.', "_").to_lowercase()),
            name: info.name,
            path: Some(launch_uri),
            steam_app_id: None,
            source: Some("xbox".to_string()),
            cover: None,
            added_at: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64),
            last_played_at: None,
            install_location: Some(info.install_location),
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
