//! Battle.net (Blizzard) 游戏库扫描

use crate::games::find_game_exe;
use crate::types::Game;
use std::path::PathBuf;

pub fn is_battlenet_installed() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if hklm
            .open_subkey(r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Battle.net")
            .is_ok()
        {
            return true;
        }
        if hklm
            .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Battle.net")
            .is_ok()
        {
            return true;
        }
    }

    let paths = [
        r"C:\Program Files (x86)\Battle.net\Battle.net.exe",
        r"C:\Program Files\Battle.net\Battle.net.exe",
    ];
    paths.iter().any(|p| std::path::Path::new(p).exists())
}

struct BnetGameDef {
    code: &'static str,
    name: &'static str,
    dirs: &'static [&'static str],
}

const KNOWN_GAMES: &[BnetGameDef] = &[
    BnetGameDef { code: "wow", name: "World of Warcraft", dirs: &["World of Warcraft", "World of Warcraft\\_retail_"] },
    BnetGameDef { code: "d3", name: "Diablo III", dirs: &["Diablo III"] },
    BnetGameDef { code: "d4", name: "Diablo IV", dirs: &["Diablo IV"] },
    BnetGameDef { code: "ow", name: "Overwatch 2", dirs: &["Overwatch"] },
    BnetGameDef { code: "hs", name: "Hearthstone", dirs: &["Hearthstone"] },
    BnetGameDef { code: "hero", name: "Heroes of the Storm", dirs: &["Heroes of the Storm"] },
    BnetGameDef { code: "sc2", name: "StarCraft II", dirs: &["StarCraft II"] },
    BnetGameDef { code: "scr", name: "StarCraft Remastered", dirs: &["StarCraft"] },
    BnetGameDef { code: "w3", name: "Warcraft III Reforged", dirs: &["Warcraft III"] },
    BnetGameDef { code: "cod", name: "Call of Duty", dirs: &["Call of Duty", "Call of Duty\\_retail_"] },
    BnetGameDef { code: "codmw", name: "Call of Duty Modern Warfare", dirs: &["Call of Duty Modern Warfare"] },
    BnetGameDef { code: "crash4", name: "Crash Bandicoot 4", dirs: &["Crash Bandicoot 4"] },
];

fn get_battlenet_install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let paths = [
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Blizzard Entertainment\Battle.net\Capabilities"),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Blizzard Entertainment\Battle.net\Capabilities"),
        ];

        for (hkey, path) in &paths {
            let root = RegKey::predef(*hkey);
            if let Ok(key) = root.open_subkey(path) {
                if let Ok(app_path) = key.get_value::<String, _>("ApplicationIcon") {
                    let p = PathBuf::from(app_path.trim_matches('"').split(',').next().unwrap_or(""));
                    if let Some(parent) = p.parent() {
                        if parent.exists() && !roots.contains(&parent.to_path_buf()) {
                            roots.push(parent.to_path_buf());
                        }
                    }
                }
            }
        }
    }

    let common = [
        r"C:\Program Files (x86)",
        r"C:\Program Files",
        r"D:\Games",
        r"E:\Games",
        r"D:\",
        r"E:\",
    ];
    for dir in &common {
        let p = PathBuf::from(dir);
        if p.exists() && !roots.contains(&p) {
            roots.push(p);
        }
    }

    roots
}

pub fn scan_battlenet_games() -> Result<Vec<Game>, String> {
    let roots = get_battlenet_install_roots();
    let mut games = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in &roots {
        for def in KNOWN_GAMES {
            for dir_name in def.dirs {
                let game_dir = root.join(dir_name);
                if !game_dir.exists() {
                    continue;
                }
                if seen.contains(def.code) {
                    continue;
                }

                let exe_path = find_game_exe(&game_dir);
                if exe_path.is_none() && !game_dir.join(".build.info").exists() && !game_dir.join(".product.db").exists() {
                    continue;
                }

                seen.insert(def.code);
                games.push(Game::new_import(
                    format!("battlenet_{}", def.code),
                    def.name.to_string(),
                    "battlenet",
                    exe_path,
                    Some(game_dir.to_string_lossy().to_string()),
                ));
            }
        }
    }

    #[cfg(windows)]
    {
        scan_battlenet_from_registry(&mut games, &mut seen);
    }

    Ok(games)
}

#[cfg(windows)]
fn scan_battlenet_from_registry(games: &mut Vec<Game>, seen: &mut std::collections::HashSet<&str>) {
    use winreg::enums::*;
    use winreg::RegKey;

    let uninstall_paths = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for path in &uninstall_paths {
        let Ok(key) = hklm.open_subkey(path) else { continue };
        let Ok(subkeys) = key.enum_keys().collect::<Result<Vec<_>, _>>() else { continue };

        for subkey_name in subkeys {
            let Ok(subkey) = key.open_subkey(&subkey_name) else { continue };
            let publisher: String = subkey.get_value("Publisher").unwrap_or_default();
            if !publisher.contains("Blizzard") && !publisher.contains("Activision") {
                continue;
            }

            let display_name: String = match subkey.get_value("DisplayName") {
                Ok(n) => n,
                Err(_) => continue,
            };
            let install_loc: String = match subkey.get_value("InstallLocation") {
                Ok(l) => l,
                Err(_) => continue,
            };

            if display_name.contains("Battle.net") || display_name.is_empty() {
                continue;
            }

            let name_lower = display_name.to_lowercase();
            let already = seen.iter().any(|code| name_lower.contains(code));
            if already { continue; }

            let install_path = PathBuf::from(&install_loc);
            if !install_path.exists() { continue; }
            let exe_path = find_game_exe(&install_path);

            let game_id = format!("battlenet_reg_{}", display_name.replace(' ', "_").to_lowercase());
            if games.iter().any(|g| g.id == game_id) { continue; }

            games.push(Game::new_import(
                game_id,
                display_name,
                "battlenet",
                exe_path,
                Some(install_loc),
            ));
        }
    }
}
