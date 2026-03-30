//! 游戏库管理模块

use arc_swap::ArcSwap;
use crate::paths::{get_covers_dir, get_games_path, get_ignored_games_path};
use crate::types::{CommandResult, Game};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::AppHandle;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

static GAMES_CACHE: OnceLock<ArcSwap<Option<Vec<Game>>>> = OnceLock::new();
static GAMES_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn get_games_cache() -> &'static ArcSwap<Option<Vec<Game>>> {
    GAMES_CACHE.get_or_init(|| ArcSwap::from_pointee(None))
}

fn get_games_write_lock() -> &'static Mutex<()> {
    GAMES_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

fn write_file_atomic(path: &std::path::Path, content: &str) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp_path = path.with_extension("tmp");
    let file = std::fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    writer.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    writer.into_inner().map_err(|e| e.to_string())?.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, path).map_err(|e| e.to_string())
}

fn resolve_shortcut(lnk_path: &str) -> Option<String> {
    #[cfg(windows)]
    {
        let safe_path = lnk_path.replace('\'', "''");
        let ps_script = format!(
            r#"
$shell = New-Object -ComObject WScript.Shell
try {{
    $shortcut = $shell.CreateShortcut('{}')
    Write-Output $shortcut.TargetPath
}} catch {{
    Write-Output ""
}}
"#,
            safe_path
        );

        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_script])
            .creation_flags(0x08000000)
            .output();

        if let Ok(output) = output {
            let target = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !target.is_empty() && std::path::Path::new(&target).exists() {
                return Some(target);
            }
        }
    }
    None
}

pub fn find_game_exe(game_dir: &PathBuf) -> Option<String> {
    if !game_dir.exists() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir(game_dir) {
        let mut exe_files: Vec<_> = entries
            .flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                name.ends_with(".exe")
                    && !name.contains("unins")
                    && !name.contains("crash")
                    && !name.contains("redist")
                    && !name.contains("vcredist")
                    && !name.contains("dxsetup")
                    && !name.contains("directx")
                    && !name.contains("easyanticheat")
                    && !name.contains("battleye")
            })
            .collect();

        exe_files.sort_by(|a, b| {
            let size_a = a.metadata().map(|m| m.len()).unwrap_or(0);
            let size_b = b.metadata().map(|m| m.len()).unwrap_or(0);
            size_b.cmp(&size_a)
        });

        if let Some(exe) = exe_files.first() {
            return Some(exe.path().to_string_lossy().to_string());
        }
    }

    None
}

pub fn load_ignored_game_ids(app: &AppHandle) -> HashSet<String> {
    let path = get_ignored_games_path(app);
    if !path.exists() {
        return HashSet::new();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };
    let ids: Vec<String> = serde_json::from_str(&content).unwrap_or_default();
    ids.into_iter().collect()
}

fn save_ignored_game_ids(app: &AppHandle, ids: &HashSet<String>) -> Result<(), String> {
    let path = get_ignored_games_path(app);
    let mut v: Vec<String> = ids.iter().cloned().collect();
    v.sort();
    let content = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    write_file_atomic(&path, &content)
}

#[cfg(windows)]
fn extract_exe_icon(exe_path: &str, save_path: &std::path::Path) -> bool {
    let ps_script = format!(
        r#"
Add-Type -AssemblyName System.Drawing
try {{
    $icon = [System.Drawing.Icon]::ExtractAssociatedIcon('{}')
    if ($icon) {{
        $bitmap = $icon.ToBitmap()
        $bitmap.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
        $icon.Dispose()
        Write-Output "OK"
    }}
}} catch {{}}
"#,
        exe_path.replace('\'', "''"),
        save_path.to_string_lossy().replace('\'', "''")
    );

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_script])
        .creation_flags(0x08000000)
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.trim() == "OK" && save_path.exists();
    }
    false
}

#[cfg(not(windows))]
fn extract_exe_icon(_exe_path: &str, _save_path: &std::path::Path) -> bool {
    false
}

// ==================== Tauri 命令 ====================

#[tauri::command]
pub async fn get_games(app: AppHandle) -> Result<Vec<Game>, String> {
    let cached = get_games_cache().load();
    if let Some(ref games) = **cached {
        return Ok(games.clone());
    }

    let path = get_games_path(&app);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let games: Vec<Game> = match serde_json::from_str(&content) {
        Ok(g) => g,
        Err(e) => {
            println!("[Games] Parse failed, returning empty: {}", e);
            vec![]
        }
    };

    get_games_cache().store(Arc::new(Some(games.clone())));
    Ok(games)
}

#[tauri::command]
pub async fn save_games(app: AppHandle, games: Vec<Game>) -> Result<(), String> {
    let _guard = get_games_write_lock().lock().unwrap_or_else(|e| e.into_inner());

    let path = get_games_path(&app);
    let content = serde_json::to_string_pretty(&games).map_err(|e| e.to_string())?;
    write_file_atomic(&path, &content)?;

    get_games_cache().store(Arc::new(Some(games)));
    Ok(())
}

#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    game_id: String,
    game_path: Option<String>,
    steam_app_id: Option<String>,
) -> CommandResult {
    // 更新最近运行时间
    if let Ok(mut games) = get_games(app.clone()).await {
        if let Some(game) = games.iter_mut().find(|g| g.id == game_id) {
            game.last_played_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            );
            let _ = save_games(app.clone(), games).await;
        }
    }

    let app_for_bg = app.clone();
    let game_id_clone = game_id.clone();
    let launch_time = std::time::Instant::now();

    if let Some(app_id) = steam_app_id {
        let url = format!("steam://rungameid/{}", app_id);
        #[cfg(windows)]
        {
            let _ = Command::new("cmd")
                .args(["/C", "start", "", &url])
                .creation_flags(0x08000000)
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("xdg-open").arg(&url).spawn();
        }
        // Steam 启动: 最小化自身，等待游戏窗口出现
        tokio::spawn(async move {
            minimize_and_wait_for_game(&app_for_bg, None, &game_id_clone, launch_time).await;
        });
        return CommandResult::success("Game launched (Steam)");
    }

    if let Some(path) = game_path {
        let path_obj = std::path::Path::new(&path);
        if path_obj.exists() {
            let mut cmd = Command::new(&path);
            if let Some(dir) = path_obj.parent() {
                cmd.current_dir(dir);
            }
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);
            match cmd.spawn() {
                Ok(child) => {
                    let pid = child.id();
                    tokio::spawn(async move {
                        minimize_and_wait_for_game(&app_for_bg, Some(pid), &game_id_clone, launch_time).await;
                    });
                }
                Err(_) => {
                    return CommandResult::error("Failed to launch game");
                }
            }
            return CommandResult::success("Game launched");
        }
        return CommandResult::error("Game file not found");
    }

    CommandResult::error("Invalid game path")
}

/// 最小化自身窗口，等待游戏窗口出现并切换到前台
async fn minimize_and_wait_for_game(app: &AppHandle, pid: Option<u32>, game_id: &str, launch_time: std::time::Instant) {
    use tauri::Manager;
    use tauri::Emitter;

    // 短暂延迟，等游戏进程启动
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    // 最小化自身窗口
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }

    #[cfg(windows)]
    {
        // 尝试将游戏窗口提到前台
        focus_game_window(pid).await;

        let game_id_owned = game_id.to_string();

        // 游戏退出后的恢复逻辑
        let on_game_exit = move |app_ref: AppHandle, start: std::time::Instant| {
            let elapsed_minutes = start.elapsed().as_secs() / 60;
            let gid = game_id_owned;

            tokio::spawn(async move {
                // 更新游戏时间
                if elapsed_minutes > 0 {
                    if let Ok(mut games) = get_games(app_ref.clone()).await {
                        if let Some(game) = games.iter_mut().find(|g| g.id == gid) {
                            let current = game.total_play_time.unwrap_or(0);
                            game.total_play_time = Some(current + elapsed_minutes as i64);
                        }
                        let _ = save_games(app_ref.clone(), games).await;
                    }
                }

                // 恢复窗口
                if let Some(window) = app_ref.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }

                // 通知前端刷新
                let _ = app_ref.emit("game-exited", ());
            });
        };

        // 后台监控游戏进程，退出后恢复窗口
        if let Some(game_pid) = pid {
            let app_clone = app.clone();
            tokio::spawn(async move {
                wait_for_process_exit(game_pid).await;
                on_game_exit(app_clone, launch_time);
            });
        } else {
            // Steam 启动无法获取 PID，延迟一段时间后恢复
            let app_clone = app.clone();
            tokio::spawn(async move {
                // 延迟检测：持续检查是否有全屏窗口，没有则恢复
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                wait_for_no_fullscreen_app().await;
                on_game_exit(app_clone, launch_time);
            });
        }
    }
}

#[cfg(windows)]
async fn focus_game_window(pid: Option<u32>) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;

    // 多次尝试聚焦，游戏可能需要时间创建窗口
    for attempt in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(if attempt == 0 { 500 } else { 1000 })).await;

        if let Some(game_pid) = pid {
            // 通过 PID 找到游戏窗口
            if let Some(hwnd) = find_window_by_pid(game_pid) {
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
                return;
            }
        }

        // 备选：检查当前前台窗口是否已经不是我们的应用
        unsafe {
            let fg = GetForegroundWindow();
            if fg != HWND::default() {
                let mut fg_pid = 0u32;
                GetWindowThreadProcessId(fg, Some(&mut fg_pid));
                let our_pid = std::process::id();
                if fg_pid != our_pid && fg_pid != 0 {
                    // 前台已经是别的应用（可能是游戏），不用做什么了
                    return;
                }
            }
        }
    }
}

#[cfg(windows)]
fn find_window_by_pid(target_pid: u32) -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;
    use std::sync::Mutex as StdMutex;

    lazy_static::lazy_static! {
        static ref FOUND_HWND: StdMutex<Option<HWND>> = StdMutex::new(None);
        static ref TARGET_PID_STORE: StdMutex<u32> = StdMutex::new(0);
    }

    // 由于 EnumWindows 回调不能捕获环境，用全局状态
    *TARGET_PID_STORE.lock().unwrap() = target_pid;
    *FOUND_HWND.lock().unwrap() = None;

    unsafe extern "system" fn enum_callback(hwnd: HWND, _: LPARAM) -> BOOL {
        let target = *TARGET_PID_STORE.lock().unwrap();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == target && IsWindowVisible(hwnd).as_bool() {
            let mut title = [0u16; 256];
            let len = GetWindowTextW(hwnd, &mut title);
            // 只匹配有标题的可见窗口
            if len > 0 {
                *FOUND_HWND.lock().unwrap() = Some(hwnd);
                return FALSE;
            }
        }
        TRUE
    }

    unsafe {
        let _ = EnumWindows(Some(enum_callback), LPARAM(0));
    }

    *FOUND_HWND.lock().unwrap()
}

#[cfg(windows)]
async fn wait_for_process_exit(pid: u32) {
    use windows::Win32::System::Threading::*;
    use windows::Win32::Foundation::*;

    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if let Ok(handle) = handle {
            loop {
                // 轮询检查进程是否还活着
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let mut exit_code = 0u32;
                let result = GetExitCodeProcess(handle, &mut exit_code);
                if result.is_ok() {
                    // STILL_ACTIVE = 259
                    if exit_code != 259 {
                        break;
                    }
                } else {
                    break;
                }
            }
            let _ = CloseHandle(handle);
        }
    }
}

#[cfg(windows)]
async fn wait_for_no_fullscreen_app() {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;

    // 每隔几秒检查前台窗口是否是全屏应用
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let is_fullscreen = unsafe {
            let fg = GetForegroundWindow();
            if fg == HWND::default() {
                false
            } else {
                let mut rect = RECT::default();
                let _ = GetWindowRect(fg, &mut rect);
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                let w = rect.right - rect.left;
                let h = rect.bottom - rect.top;
                // 如果前台窗口几乎覆盖整个屏幕，认为是全屏游戏
                w >= screen_w - 10 && h >= screen_h - 10
            }
        };

        if !is_fullscreen {
            break;
        }
    }
}

#[tauri::command]
pub async fn delete_game(app: AppHandle, game_id: String) -> Result<(), String> {
    let _guard = get_games_write_lock().lock().unwrap_or_else(|e| e.into_inner());

    let path = get_games_path(&app);
    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let games: Vec<Game> = serde_json::from_str(&content).unwrap_or_default();
        let filtered: Vec<Game> = games.into_iter().filter(|g| g.id != game_id).collect();

        let new_content = serde_json::to_string_pretty(&filtered).map_err(|e| e.to_string())?;
        write_file_atomic(&path, &new_content)?;

        get_games_cache().store(Arc::new(Some(filtered)));
    }

    let mut ignored = load_ignored_game_ids(&app);
    ignored.insert(game_id);
    let _ = save_ignored_game_ids(&app, &ignored);

    Ok(())
}

#[tauri::command]
pub async fn select_game_exe(app: AppHandle) -> Result<Option<Game>, String> {
    let app_clone = app.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<Option<Game>, String> {
        use tauri_plugin_dialog::DialogExt;

        let file_path = app_clone
            .dialog()
            .file()
            .add_filter("Executables", &["exe", "bat", "cmd", "lnk"])
            .add_filter("All files", &["*"])
            .blocking_pick_file();

        if let Some(path) = file_path {
            let path_str = path.to_string();
            let path_lower = path_str.to_lowercase();

            let (actual_path, display_name) = if path_lower.ends_with(".lnk") {
                match resolve_shortcut(&path_str) {
                    Some(target) => {
                        let lnk_name = std::path::Path::new(&path_str)
                            .file_stem()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        (target, lnk_name)
                    }
                    None => return Err("Cannot resolve shortcut".to_string()),
                }
            } else {
                let file_name = std::path::Path::new(&path_str)
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                (path_str.clone(), file_name)
            };

            let game_id = format!(
                "manual_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            );

            let mut cover_path: Option<String> = None;
            let covers_dir = get_covers_dir(&app_clone);
            let _ = std::fs::create_dir_all(&covers_dir);
            let icon_save_path = covers_dir.join(format!("{}.png", &game_id));

            if extract_exe_icon(&actual_path, &icon_save_path) {
                cover_path = Some(icon_save_path.to_string_lossy().to_string());
            }

            let install_loc = std::path::Path::new(&actual_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string());

            let new_game = Game {
                id: game_id,
                name: display_name,
                path: Some(actual_path),
                steam_app_id: None,
                source: Some("manual".to_string()),
                cover: cover_path,
                added_at: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64,
                ),
                last_played_at: None,
                install_location: install_loc,
                pinned: None,
                completion_status: None,
                total_play_time: None,
                description: None,
                genre: None,
                release_year: None,
                favorite: None,
            };

            return Ok(Some(new_game));
        }

        Ok(None)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    if let Some(new_game) = result {
        let mut games = get_games(app.clone()).await.unwrap_or_default();
        games.push(new_game.clone());
        let _ = save_games(app, games).await;
        return Ok(Some(new_game));
    }

    Ok(None)
}

// ==================== Steam 导入 ====================

fn scan_steam_games_sync() -> Result<Vec<Game>, String> {
    use crate::paths::get_all_steam_library_folders;
    use std::collections::HashSet;

    let mut games = vec![];
    let mut seen_app_ids: HashSet<String> = HashSet::new();

    let library_folders = get_all_steam_library_folders();
    if library_folders.is_empty() {
        return Err("Steam not found".to_string());
    }

    for steamapps in &library_folders {
        if let Ok(entries) = std::fs::read_dir(steamapps) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("appmanifest_") && name.ends_with(".acf") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        let get_value = |key: &str, content: &str| -> Option<String> {
                            let pattern = format!("\"{}\"\\s+\"([^\"]+)\"", key);
                            let re = regex::Regex::new(&pattern).ok()?;
                            re.captures(content)
                                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                        };

                        if let (Some(app_id), Some(game_name), Some(install_dir)) = (
                            get_value("appid", &content),
                            get_value("name", &content),
                            get_value("installdir", &content),
                        ) {
                            if game_name.contains("Proton")
                                || game_name.contains("Redistributable")
                            {
                                continue;
                            }

                            if seen_app_ids.contains(&app_id) {
                                continue;
                            }
                            seen_app_ids.insert(app_id.clone());

                            let game_path = steamapps.join("common").join(&install_dir);
                            let exe_path = find_game_exe(&game_path);

                            games.push(Game {
                                id: format!("steam_{}", app_id),
                                name: game_name,
                                path: exe_path,
                                steam_app_id: Some(app_id),
                                source: Some("steam".to_string()),
                                cover: None,
                                added_at: Some(
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as i64,
                                ),
                                last_played_at: None,
                                install_location: Some(
                                    game_path.to_string_lossy().to_string(),
                                ),
                                pinned: None,
                                completion_status: None,
                                total_play_time: None,
                                description: None,
                                genre: None,
                                release_year: None,
                                favorite: None,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(games)
}

#[tauri::command]
pub async fn import_steam_games(app: AppHandle) -> Result<Vec<Game>, String> {
    let games = tokio::task::spawn_blocking(scan_steam_games_sync)
        .await
        .map_err(|e| format!("Task failed: {}", e))??;

    let ignored_ids = load_ignored_game_ids(&app);
    let existing = get_games(app.clone()).await.unwrap_or_default();
    let existing_ids: HashSet<_> = existing.iter().map(|g| g.id.clone()).collect();

    let new_games: Vec<_> = games
        .into_iter()
        .filter(|g| !existing_ids.contains(&g.id))
        .filter(|g| !ignored_ids.contains(&g.id))
        .collect();

    if !new_games.is_empty() {
        let mut all_games = existing;
        all_games.extend(new_games.clone());
        let _ = save_games(app, all_games).await;
    }

    Ok(new_games)
}

// ==================== Steam 封面 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn get_steam_video_url(steam_app_id: String) -> serde_json::Value {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return serde_json::json!({ "success": false }),
    };

    let hero_urls = vec![
        format!(
            "https://steamcdn-a.akamaihd.net/steam/apps/{}/library_hero.jpg",
            steam_app_id
        ),
        format!(
            "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/library_hero.jpg",
            steam_app_id
        ),
    ];

    for url in &hero_urls {
        match client.head(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                return serde_json::json!({
                    "success": true,
                    "heroUrl": url,
                    "steamAppId": steam_app_id
                });
            }
            _ => continue,
        }
    }

    let api_url = format!(
        "https://store.steampowered.com/api/appdetails?appids={}",
        steam_app_id
    );

    match client.get(&api_url).send().await {
        Ok(resp) => {
            if let Ok(text) = resp.text().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(app_data) = json.get(&steam_app_id) {
                        if app_data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                            if let Some(data) = app_data.get("data") {
                                if let Some(bg) = data.get("background").and_then(|v| v.as_str()) {
                                    return serde_json::json!({
                                        "success": true,
                                        "heroUrl": bg,
                                        "steamAppId": steam_app_id
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(_) => {}
    }

    serde_json::json!({ "success": false })
}

// ==================== 游戏元数据 ====================

/// 从 Steam Store API 拉取游戏元数据（描述、类型、发行年份）
#[tauri::command(rename_all = "camelCase")]
pub async fn fetch_steam_metadata(app: AppHandle, game_id: String, steam_app_id: String) -> CommandResult<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "https://store.steampowered.com/api/appdetails?appids={}&l=schinese",
        steam_app_id
    );

    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    let mut description: Option<String> = None;
    let mut genre: Option<String> = None;
    let mut release_year: Option<i32> = None;

    if let Some(app_data) = json.get(&steam_app_id) {
        if app_data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            if let Some(data) = app_data.get("data") {
                description = data.get("short_description").and_then(|v| v.as_str()).map(|s| s.to_string());
                
                if let Some(genres) = data.get("genres").and_then(|v| v.as_array()) {
                    let genre_names: Vec<&str> = genres
                        .iter()
                        .filter_map(|g| g.get("description").and_then(|d| d.as_str()))
                        .take(3)
                        .collect();
                    if !genre_names.is_empty() {
                        genre = Some(genre_names.join(", "));
                    }
                }

                if let Some(release) = data.get("release_date").and_then(|v| v.get("date")).and_then(|v| v.as_str()) {
                    // 尝试从日期字符串中提取年份（格式可能是 "2023 年 1 月 1 日" 或 "Jan 1, 2023"）
                    let re = regex::Regex::new(r"(\d{4})").ok();
                    if let Some(re) = re {
                        if let Some(cap) = re.captures(release) {
                            if let Ok(year) = cap[1].parse::<i32>() {
                                if year >= 1970 && year <= 2100 {
                                    release_year = Some(year);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 更新游戏数据
    let mut updated = false;
    if let Ok(mut games) = get_games(app.clone()).await {
        if let Some(game) = games.iter_mut().find(|g| g.id == game_id) {
            if description.is_some() && game.description.is_none() {
                game.description = description.clone();
                updated = true;
            }
            if genre.is_some() && game.genre.is_none() {
                game.genre = genre.clone();
                updated = true;
            }
            if release_year.is_some() && game.release_year.is_none() {
                game.release_year = release_year;
                updated = true;
            }
        }
        if updated {
            let _ = save_games(app, games).await;
        }
    }

    Ok(serde_json::json!({
        "description": description,
        "genre": genre,
        "releaseYear": release_year,
        "updated": updated
    }))
}

/// 更新单个游戏的字段
#[tauri::command(rename_all = "camelCase")]
pub async fn update_game(app: AppHandle, game_id: String, updates: serde_json::Value) -> Result<(), String> {
    let mut games = get_games(app.clone()).await.map_err(|e| format!("{:?}", e))?;
    
    if let Some(game) = games.iter_mut().find(|g| g.id == game_id) {
        if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
            game.name = name.to_string();
        }
        if let Some(cover) = updates.get("cover").and_then(|v| v.as_str()) {
            game.cover = Some(cover.to_string());
        }
        if let Some(desc) = updates.get("description").and_then(|v| v.as_str()) {
            game.description = Some(desc.to_string());
        }
        if let Some(genre) = updates.get("genre").and_then(|v| v.as_str()) {
            game.genre = Some(genre.to_string());
        }
        if let Some(year) = updates.get("releaseYear").and_then(|v| v.as_i64()) {
            game.release_year = Some(year as i32);
        }
        if let Some(status) = updates.get("completionStatus").and_then(|v| v.as_str()) {
            game.completion_status = Some(status.to_string());
        }
        if let Some(fav) = updates.get("favorite").and_then(|v| v.as_bool()) {
            game.favorite = Some(fav);
        }
        if let Some(pinned) = updates.get("pinned").and_then(|v| v.as_bool()) {
            game.pinned = Some(pinned);
        }
        
        save_games(app, games).await.map_err(|e| format!("{:?}", e))?;
    } else {
        return Err("Game not found".to_string());
    }

    Ok(())
}

/// 选择自定义封面图片
#[tauri::command]
pub async fn select_cover_image(app: AppHandle, game_id: String) -> Result<Option<String>, String> {
    let app_clone = app.clone();
    let game_id_clone = game_id.clone();
    
    let result = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        use tauri_plugin_dialog::DialogExt;

        let file_path = app_clone
            .dialog()
            .file()
            .add_filter("Images", &["jpg", "jpeg", "png", "webp", "bmp", "gif"])
            .blocking_pick_file();

        if let Some(path) = file_path {
            let path_str = path.to_string();
            let covers_dir = get_covers_dir(&app_clone);
            let _ = std::fs::create_dir_all(&covers_dir);
            
            let ext = std::path::Path::new(&path_str)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg");
            let dest = covers_dir.join(format!("{}.{}", game_id_clone, ext));
            
            std::fs::copy(&path_str, &dest).map_err(|e| e.to_string())?;
            return Ok(Some(dest.to_string_lossy().to_string()));
        }

        Ok(None)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    if let Some(ref cover_path) = result {
        let mut games = get_games(app.clone()).await.map_err(|e| format!("{:?}", e))?;
        if let Some(game) = games.iter_mut().find(|g| g.id == game_id) {
            game.cover = Some(cover_path.clone());
        }
        save_games(app, games).await.map_err(|e| format!("{:?}", e))?;
    }

    Ok(result)
}

// ==================== 游戏路径与存档 ====================

/// 在系统文件管理器中打开文件夹
#[tauri::command(rename_all = "camelCase")]
pub async fn open_folder(folder_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&folder_path);
    if !path.exists() {
        return Err("Folder not found".to_string());
    }
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg(&folder_path)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 查找游戏的存档路径
#[tauri::command(rename_all = "camelCase")]
pub async fn find_save_paths(game_name: String, steam_app_id: Option<String>) -> Result<Vec<serde_json::Value>, String> {
    let mut results: Vec<serde_json::Value> = Vec::new();

    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();

    // 清理游戏名，用于目录匹配
    let clean_name = game_name
        .replace([':', '™', '®', '©', '!', '?', '.', ','], "")
        .trim()
        .to_string();
    let name_lower = clean_name.to_lowercase();
    // 提取关键词(至少3字符)
    let keywords: Vec<String> = name_lower
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_string())
        .collect();

    // 搜索常见存档位置
    let search_dirs = vec![
        (format!("{}\\Documents\\My Games", home), "My Games"),
        (format!("{}\\Saved Games", home), "Saved Games"),
        (format!("{}\\Documents", home), "Documents"),
        (appdata.clone(), "AppData/Roaming"),
        (format!("{}\\Low", localappdata), "AppData/LocalLow"),
        (localappdata.clone(), "AppData/Local"),
    ];

    for (dir, label) in &search_dirs {
        let dir_path = std::path::Path::new(dir);
        if !dir_path.exists() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let folder_name = entry.file_name().to_string_lossy().to_lowercase();
                
                // 匹配：文件夹名包含任意关键词
                let matched = keywords.iter().any(|kw| folder_name.contains(kw));
                if matched {
                    results.push(serde_json::json!({
                        "path": entry.path().to_string_lossy(),
                        "label": label,
                        "type": "save"
                    }));
                }
            }
        }
    }

    // Steam userdata 存档
    if let Some(ref app_id) = steam_app_id {
        if let Some(steam_path) = crate::paths::get_steam_install_path() {
            let userdata = steam_path.join("userdata");
            if userdata.exists() {
                if let Ok(users) = std::fs::read_dir(&userdata) {
                    for user_entry in users.flatten() {
                        let save_dir = user_entry.path().join(app_id);
                        if save_dir.exists() {
                            results.push(serde_json::json!({
                                "path": save_dir.to_string_lossy(),
                                "label": "Steam Cloud",
                                "type": "steam_cloud"
                            }));
                        }
                    }
                }
            }
        }
    }

    // 去重
    let mut seen = std::collections::HashSet::new();
    results.retain(|r| {
        let p = r.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
        seen.insert(p)
    });

    Ok(results)
}

// ==================== Steam 封面下载 ====================

use std::path::Path;

fn get_steam_paths() -> Vec<PathBuf> {
    use crate::paths::get_steam_install_path;
    match get_steam_install_path() {
        Some(p) => vec![p],
        None => vec![],
    }
}

fn copy_local_steam_cover(app_id: &str, covers_dir: &Path, game_id: &str) -> Option<PathBuf> {
    let candidates = [
        "library_600x900_2x",
        "library_600x900",
        "library_hero",
        "header",
    ];
    let extensions = ["jpg", "jpeg", "png", "webp"];

    for steam_path in get_steam_paths() {
        let cache_dir = steam_path.join("appcache").join("librarycache");
        if !cache_dir.exists() {
            continue;
        }

        for key in candidates {
            for ext in extensions {
                let local_file = cache_dir.join(format!("{}_{}.{}", app_id, key, ext));
                if !local_file.exists() {
                    continue;
                }

                let dest = covers_dir.join(format!("{}.{}", game_id, ext));
                match std::fs::copy(&local_file, &dest) {
                    Ok(_) => return Some(dest),
                    Err(_) => {}
                }
            }
        }
    }

    None
}

async fn download_steam_cover_async(
    app_id: &str,
    covers_dir: &Path,
    game_id: &str,
) -> Option<PathBuf> {
    // 先尝试本地 Steam 缓存
    if let Some(p) = copy_local_steam_cover(app_id, covers_dir, game_id) {
        return Some(p);
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let dest_path = covers_dir.join(format!("{}.jpg", game_id));

    let urls = vec![
        format!("https://steamcdn-a.akamaihd.net/steam/apps/{}/library_600x900_2x.jpg", app_id),
        format!("https://steamcdn-a.akamaihd.net/steam/apps/{}/library_600x900.jpg", app_id),
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg", app_id),
    ];

    for url in &urls {
        match client.get(url).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(bytes) = response.bytes().await {
                    if bytes.len() > 1000 {
                        if std::fs::write(&dest_path, &bytes).is_ok() {
                            return Some(dest_path);
                        }
                    }
                }
            }
            _ => continue,
        }
    }

    None
}

fn find_existing_cover(covers_dir: &Path, game_id: &str) -> Option<PathBuf> {
    let extensions = ["jpg", "jpeg", "png", "webp", "bmp", "gif"];
    for ext in extensions {
        let path = covers_dir.join(format!("{}.{}", game_id, ext));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// 批量下载 Steam 封面
#[tauri::command]
pub async fn download_steam_covers(app: AppHandle) -> CommandResult<serde_json::Value> {
    let games = get_games(app.clone()).await.unwrap_or_default();
    let covers_dir = get_covers_dir(&app);
    let _ = std::fs::create_dir_all(&covers_dir);

    let steam_games: Vec<_> = games
        .iter()
        .filter(|g| g.steam_app_id.is_some() && g.cover.is_none())
        .filter(|g| find_existing_cover(&covers_dir, &g.id).is_none())
        .collect();

    let total = steam_games.len();
    let mut downloaded = 0u32;
    let mut updated_games = get_games(app.clone()).await.unwrap_or_default();

    for game in &steam_games {
        let app_id = game.steam_app_id.as_ref().unwrap();
        if let Some(cover_path) = download_steam_cover_async(app_id, &covers_dir, &game.id).await {
            let cover_str = cover_path.to_string_lossy().to_string();
            if let Some(g) = updated_games.iter_mut().find(|g| g.id == game.id) {
                g.cover = Some(cover_str);
            }
            downloaded += 1;
        }
    }

    if downloaded > 0 {
        let _ = save_games(app, updated_games).await;
    }

    Ok(serde_json::json!({
        "total": total,
        "downloaded": downloaded
    }))
}

/// 下载单个游戏封面
#[tauri::command(rename_all = "camelCase")]
pub async fn download_game_cover(app: AppHandle, game_id: String) -> CommandResult<Option<String>> {
    let games = get_games(app.clone()).await.unwrap_or_default();
    let game = games.iter().find(|g| g.id == game_id);
    let game = match game {
        Some(g) => g,
        None => return Ok(None),
    };

    let covers_dir = get_covers_dir(&app);
    let _ = std::fs::create_dir_all(&covers_dir);

    // 已有封面直接返回
    if let Some(existing) = find_existing_cover(&covers_dir, &game.id) {
        return Ok(Some(existing.to_string_lossy().to_string()));
    }

    // Steam 游戏才能下载
    let app_id = match &game.steam_app_id {
        Some(id) => id.clone(),
        None => return Ok(None),
    };

    if let Some(cover_path) = download_steam_cover_async(&app_id, &covers_dir, &game.id).await {
        let cover_str = cover_path.to_string_lossy().to_string();
        let mut all_games = get_games(app.clone()).await.unwrap_or_default();
        if let Some(g) = all_games.iter_mut().find(|g| g.id == game_id) {
            g.cover = Some(cover_str.clone());
        }
        let _ = save_games(app, all_games).await;
        Ok(Some(cover_str))
    } else {
        Ok(None)
    }
}
