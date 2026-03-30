//! 路径管理模块

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Manager;

lazy_static::lazy_static! {
    static ref APP_DATA_DIR_CACHE: Mutex<Option<PathBuf>> = Mutex::new(None);
}

/// 获取应用数据基础目录
fn get_app_data_base(app: &AppHandle) -> PathBuf {
    {
        let cache = APP_DATA_DIR_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref dir) = *cache {
            return dir.clone();
        }
    }

    // 使用 LOCALAPPDATA/com.kogames
    let dir = if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("com.kogames")
    } else {
        app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let _ = std::fs::create_dir_all(&dir);

    let mut cache = APP_DATA_DIR_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *cache = Some(dir.clone());
    dir
}

pub fn get_games_path(app: &AppHandle) -> PathBuf {
    get_app_data_base(app).join("game_library.json")
}

pub fn get_ignored_games_path(app: &AppHandle) -> PathBuf {
    get_app_data_base(app).join("ignored_games.json")
}

pub fn get_covers_dir(app: &AppHandle) -> PathBuf {
    get_app_data_base(app).join("covers")
}

/// 获取 Steam 主安装目录（通过注册表）
pub fn get_steam_install_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let normalize_path = |s: &str| -> PathBuf {
            let trimmed = s.trim().trim_matches('"');
            let normalized = trimmed.replace('/', "\\");
            PathBuf::from(normalized)
        };

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey(r"SOFTWARE\Valve\Steam") {
            if let Ok(path) = key
                .get_value::<String, _>("SteamPath")
                .or_else(|_| key.get_value::<String, _>("InstallPath"))
            {
                let p = normalize_path(&path);
                if p.exists() {
                    return Some(p);
                }
            }

            if let Ok(exe) = key.get_value::<String, _>("SteamExe") {
                let exe_path = normalize_path(&exe);
                if let Some(dir) = exe_path.parent() {
                    if dir.exists() {
                        return Some(dir.to_path_buf());
                    }
                }
            }
        }

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let steam_key = hklm
            .open_subkey(r"SOFTWARE\WOW6432Node\Valve\Steam")
            .or_else(|_| hklm.open_subkey(r"SOFTWARE\Valve\Steam"));

        if let Ok(key) = steam_key {
            if let Ok(path) = key
                .get_value::<String, _>("InstallPath")
                .or_else(|_| key.get_value::<String, _>("SteamPath"))
            {
                let p = normalize_path(&path);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    let common_paths = vec![
        PathBuf::from("C:\\Program Files (x86)\\Steam"),
        PathBuf::from("C:\\Program Files\\Steam"),
    ];

    common_paths.into_iter().find(|path| path.exists())
}

/// 获取所有 Steam 游戏库文件夹路径
pub fn get_all_steam_library_folders() -> Vec<PathBuf> {
    let mut libraries = Vec::new();

    let steam_path = match get_steam_install_path() {
        Some(p) => p,
        None => return libraries,
    };

    let main_library = steam_path.join("steamapps");
    if main_library.exists() {
        libraries.push(main_library);
    }

    let vdf_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    if !vdf_path.exists() {
        return libraries;
    }

    if let Ok(content) = std::fs::read_to_string(&vdf_path) {
        let path_regex = regex::Regex::new(r#""path"\s+"([^"]+)""#).ok();

        if let Some(re) = path_regex {
            for cap in re.captures_iter(&content) {
                if let Some(path_match) = cap.get(1) {
                    let path_str = path_match.as_str().replace("\\\\", "\\");
                    let library_path = PathBuf::from(path_str).join("steamapps");

                    if library_path.exists() && !libraries.contains(&library_path) {
                        libraries.push(library_path);
                    }
                }
            }
        }
    }

    libraries
}
