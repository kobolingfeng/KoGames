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
static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .pool_max_idle_per_host(5)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

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

fn run_script(script: &str) {
    #[cfg(windows)]
    {
        let _ = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
            .creation_flags(0x08000000)
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("sh").arg("-c").arg(script).output();
    }
}

#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    game_id: String,
    game_path: Option<String>,
    steam_app_id: Option<String>,
    action_index: Option<usize>,
) -> CommandResult {
    let game_data = get_games(app.clone()).await.unwrap_or_default();
    let game_ref = game_data.iter().find(|g| g.id == game_id);

    if let Some(game) = game_ref {
        if let Some(ref script) = game.pre_script {
            if !script.trim().is_empty() {
                run_script(script);
            }
        }
    }

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

    let post_script = game_ref.and_then(|g| g.post_script.clone());
    let app_for_bg = app.clone();
    let game_id_clone = game_id.clone();
    let launch_time = std::time::Instant::now();

    let resolved_path = if let Some(idx) = action_index {
        game_ref.and_then(|g| g.game_actions.as_ref())
            .and_then(|actions| actions.get(idx))
            .map(|a| a.path.clone())
    } else {
        game_path.clone()
    };

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
        tokio::spawn(async move {
            minimize_and_wait_for_game(&app_for_bg, None, &game_id_clone, launch_time).await;
            if let Some(ref script) = post_script {
                if !script.trim().is_empty() { run_script(script); }
            }
        });
        return CommandResult::success("Game launched (Steam)");
    }

    if let Some(path) = resolved_path.or(game_path) {
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
                        if let Some(ref script) = post_script {
                            if !script.trim().is_empty() { run_script(script); }
                        }
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
        let session_start_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let on_game_exit = move |app_ref: AppHandle, start: std::time::Instant| {
            let elapsed_minutes = start.elapsed().as_secs() / 60;
            let gid = game_id_owned;
            let sess_start = session_start_ms;

            tokio::spawn(async move {
                if let Ok(mut games) = get_games(app_ref.clone()).await {
                    if let Some(game) = games.iter_mut().find(|g| g.id == gid) {
                        if elapsed_minutes > 0 {
                            let current = game.total_play_time.unwrap_or(0);
                            game.total_play_time = Some(current + elapsed_minutes as i64);
                        }

                        game.play_count = Some(game.play_count.unwrap_or(0) + 1);

                        let session_end = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as i64;
                        let session = crate::types::GameSession {
                            started_at: sess_start,
                            ended_at: session_end,
                            duration_minutes: elapsed_minutes as i64,
                        };
                        let sessions = game.sessions.get_or_insert_with(Vec::new);
                        sessions.push(session);
                        const MAX_SESSIONS: usize = 500;
                        if sessions.len() > MAX_SESSIONS {
                            let drain_count = sessions.len() - MAX_SESSIONS;
                            sessions.drain(..drain_count);
                        }
                    }
                    let _ = save_games(app_ref.clone(), games).await;
                }

                if let Some(window) = app_ref.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }

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
fn find_window_by_pid(target_pid: u32) -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;
    use std::sync::atomic::{AtomicU32, AtomicIsize, Ordering};

    static TARGET_PID: AtomicU32 = AtomicU32::new(0);
    static FOUND_HWND_RAW: AtomicIsize = AtomicIsize::new(0);

    TARGET_PID.store(target_pid, Ordering::SeqCst);
    FOUND_HWND_RAW.store(0, Ordering::SeqCst);

    unsafe extern "system" fn enum_callback(hwnd: HWND, _: LPARAM) -> BOOL {
        let target = TARGET_PID.load(Ordering::SeqCst);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == target && IsWindowVisible(hwnd).as_bool() {
            let mut title = [0u16; 256];
            let len = GetWindowTextW(hwnd, &mut title);
            if len > 0 {
                FOUND_HWND_RAW.store(hwnd.0 as isize, Ordering::SeqCst);
                return FALSE;
            }
        }
        TRUE
    }

    unsafe {
        let _ = EnumWindows(Some(enum_callback), LPARAM(0));
    }

    let raw = FOUND_HWND_RAW.load(Ordering::SeqCst);
    if raw != 0 { Some(HWND(raw as *mut _)) } else { None }
}

#[cfg(windows)]
async fn wait_for_process_exit(pid: u32) {
    use windows::Win32::System::Threading::*;
    use windows::Win32::Foundation::*;

    #[derive(Clone, Copy)]
    struct SendHandle(isize);
    unsafe impl Send for SendHandle {}

    let handle_raw = unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => SendHandle(h.0 as isize),
            Err(_) => return,
        }
    };

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let sh = handle_raw;
        let exited = unsafe {
            let handle = HANDLE(sh.0 as *mut _);
            let mut exit_code = 0u32;
            let result = GetExitCodeProcess(handle, &mut exit_code);
            if result.is_ok() { exit_code != 259 } else { true }
        };
        if exited { break; }
    }

    unsafe {
        let _ = CloseHandle(HANDLE(handle_raw.0 as *mut _));
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

            let mut new_game = Game::new_import(
                game_id,
                display_name,
                "manual",
                Some(actual_path),
                install_loc,
            );
            new_game.cover = cover_path;

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

                            let mut game = Game::new_import(
                                format!("steam_{}", app_id),
                                game_name,
                                "steam",
                                exe_path,
                                Some(game_path.to_string_lossy().to_string()),
                            );
                            game.steam_app_id = Some(app_id);
                            games.push(game);
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
    let client = get_http_client();

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

/// 从 Steam Store API 拉取游戏元数据
#[tauri::command(rename_all = "camelCase")]
pub async fn fetch_steam_metadata(app: AppHandle, game_id: String, steam_app_id: String) -> Result<serde_json::Value, String> {
    let client = get_http_client();

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
    let mut release_date: Option<String> = None;
    let mut developers: Option<Vec<String>> = None;
    let mut publishers: Option<Vec<String>> = None;
    let mut categories_list: Option<Vec<String>> = None;
    let mut critic_score: Option<f32> = None;
    let mut background_image: Option<String> = None;
    let mut movies: Vec<serde_json::Value> = Vec::new();

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

                if let Some(devs) = data.get("developers").and_then(|v| v.as_array()) {
                    let dev_names: Vec<String> = devs.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    if !dev_names.is_empty() { developers = Some(dev_names); }
                }

                if let Some(pubs) = data.get("publishers").and_then(|v| v.as_array()) {
                    let pub_names: Vec<String> = pubs.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    if !pub_names.is_empty() { publishers = Some(pub_names); }
                }

                if let Some(cats) = data.get("categories").and_then(|v| v.as_array()) {
                    let cat_names: Vec<String> = cats.iter().filter_map(|c| c.get("description").and_then(|d| d.as_str()).map(|s| s.to_string())).collect();
                    if !cat_names.is_empty() { categories_list = Some(cat_names); }
                }

                if let Some(mc) = data.get("metacritic").and_then(|v| v.get("score")).and_then(|v| v.as_f64()) {
                    critic_score = Some(mc as f32);
                }

                background_image = data.get("background").and_then(|v| v.as_str()).map(|s| s.to_string());

                // 提取视频/预告片
                if let Some(movie_arr) = data.get("movies").and_then(|v| v.as_array()) {
                    println!("[VIDEO] Found {} movies in Steam API for appid {}", movie_arr.len(), steam_app_id);
                    for m in movie_arr {
                        let mut movie = serde_json::Map::new();
                        if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                            movie.insert("name".into(), serde_json::json!(name));
                        }
                        if let Some(thumb) = m.get("thumbnail").and_then(|v| v.as_str()) {
                            movie.insert("thumbnail".into(), serde_json::json!(thumb));
                        }

                        // HLS 流 (用 hls.js 在前端播放)
                        if let Some(hls) = m.get("hls_h264").and_then(|v| v.as_str()) {
                            movie.insert("hlsUrl".into(), serde_json::json!(hls));
                        }

                        // 旧格式 mp4 直链 (兼容老游戏)
                        if let Some(movie_id) = m.get("id").and_then(|v| v.as_u64()) {
                            let mp4_max = format!("https://steamcdn-a.akamaihd.net/steam/apps/{}/movie_max.mp4", movie_id);
                            movie.insert("mp4Max".into(), serde_json::json!(mp4_max));
                        }

                        if !movie.is_empty() {
                            movies.push(serde_json::Value::Object(movie));
                        }
                    }
                } else {
                    println!("[VIDEO] No 'movies' field in Steam API response for appid {}", steam_app_id);
                }

                if let Some(release) = data.get("release_date").and_then(|v| v.get("date")).and_then(|v| v.as_str()) {
                    release_date = Some(release.to_string());
                    let re = regex::Regex::new(r"(\d{4})").ok();
                    if let Some(re) = re {
                        if let Some(cap) = re.captures(release) {
                            if let Ok(year) = cap[1].parse::<i32>() {
                                if (1970..=2100).contains(&year) {
                                    release_year = Some(year);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut updated = false;
    if let Ok(mut games) = get_games(app.clone()).await {
        if let Some(game) = games.iter_mut().find(|g| g.id == game_id) {
            macro_rules! fill {
                ($game_field:ident, $src:ident) => {
                    if $src.is_some() && game.$game_field.is_none() {
                        game.$game_field = $src.clone();
                        updated = true;
                    }
                };
            }
            fill!(description, description);
            fill!(genre, genre);
            fill!(release_year, release_year);
            fill!(release_date, release_date);
            fill!(developers, developers);
            fill!(publishers, publishers);
            fill!(categories, categories_list);
            fill!(critic_score, critic_score);
            fill!(background_image, background_image);
        }
        if updated {
            let _ = save_games(app, games).await;
        }
    }

    Ok(serde_json::json!({
        "description": description,
        "genre": genre,
        "releaseYear": release_year,
        "releaseDate": release_date,
        "developers": developers,
        "publishers": publishers,
        "categories": categories_list,
        "criticScore": critic_score,
        "backgroundImage": background_image,
        "movies": movies,
        "updated": updated
    }))
}

/// 从 Steam API 获取游戏视频/预告片 URL
#[tauri::command(rename_all = "camelCase")]
pub async fn fetch_steam_videos(steam_app_id: String) -> Result<serde_json::Value, String> {
    let client = get_http_client();
    let url = format!(
        "https://store.steampowered.com/api/appdetails?appids={}&filters=movies",
        steam_app_id
    );
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    let mut videos: Vec<serde_json::Value> = Vec::new();
    if let Some(app_data) = json.get(&steam_app_id) {
        if app_data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            if let Some(data) = app_data.get("data") {
                if let Some(movie_arr) = data.get("movies").and_then(|v| v.as_array()) {
                    for m in movie_arr {
                        let thumbnail = m.get("thumbnail").and_then(|v| v.as_str()).unwrap_or("");
                        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let hls_url = m.get("hls_h264").and_then(|v| v.as_str()).unwrap_or("");
                        let mut mp4_max = String::new();
                        if let Some(movie_id) = m.get("id").and_then(|v| v.as_u64()) {
                            mp4_max = format!("https://steamcdn-a.akamaihd.net/steam/apps/{}/movie_max.mp4", movie_id);
                        }
                        videos.push(serde_json::json!({
                            "name": name,
                            "thumbnail": thumbnail,
                            "hlsUrl": hls_url,
                            "mp4Max": mp4_max
                        }));
                    }
                }
            }
        }
    }
    Ok(serde_json::json!({ "videos": videos }))
}

/// 更新单个游戏的字段
#[tauri::command(rename_all = "camelCase")]
pub async fn update_game(app: AppHandle, game_id: String, updates: serde_json::Value) -> Result<(), String> {
    let mut games = get_games(app.clone()).await.map_err(|e| format!("{:?}", e))?;
    
    if let Some(game) = games.iter_mut().find(|g| g.id == game_id) {
        macro_rules! set_str {
            ($field:ident, $key:expr) => {
                if let Some(v) = updates.get($key).and_then(|v| v.as_str()) {
                    game.$field = Some(v.to_string());
                }
            };
        }
        macro_rules! set_bool {
            ($field:ident, $key:expr) => {
                if let Some(v) = updates.get($key).and_then(|v| v.as_bool()) {
                    game.$field = Some(v);
                }
            };
        }
        macro_rules! set_str_vec {
            ($field:ident, $key:expr) => {
                if let Some(arr) = updates.get($key).and_then(|v| v.as_array()) {
                    let items: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    if !items.is_empty() {
                        game.$field = Some(items);
                    }
                }
            };
        }

        if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
            game.name = name.to_string();
        }

        set_str!(cover, "cover");
        set_str!(background_image, "backgroundImage");
        set_str!(icon, "icon");
        set_str!(logo, "logo");
        set_str!(description, "description");
        set_str!(genre, "genre");
        set_str!(completion_status, "completionStatus");
        set_str!(release_date, "releaseDate");
        set_str!(series, "series");
        set_str!(age_rating, "ageRating");
        set_str!(notes, "notes");
        set_str!(pre_script, "preScript");
        set_str!(post_script, "postScript");
        set_str!(platform, "platform");
        set_str!(purchase_date, "purchaseDate");
        set_str!(purchase_store, "purchaseStore");

        set_bool!(favorite, "favorite");
        set_bool!(pinned, "pinned");
        set_bool!(hidden, "hidden");

        if let Some(rating) = updates.get("userRating").and_then(|v| v.as_f64()) {
            game.user_rating = Some(rating as f32);
        }
        if let Some(price) = updates.get("purchasePrice").and_then(|v| v.as_f64()) {
            game.purchase_price = Some(price);
        }

        if let Some(year) = updates.get("releaseYear").and_then(|v| v.as_i64()) {
            game.release_year = Some(year as i32);
        }
        if let Some(score) = updates.get("criticScore").and_then(|v| v.as_f64()) {
            game.critic_score = Some(score as f32);
        }
        if let Some(score) = updates.get("communityScore").and_then(|v| v.as_f64()) {
            game.community_score = Some(score as f32);
        }
        if let Some(size) = updates.get("installSize").and_then(|v| v.as_u64()) {
            game.install_size = Some(size);
        }

        set_str_vec!(developers, "developers");
        set_str_vec!(publishers, "publishers");
        set_str_vec!(tags, "tags");
        set_str_vec!(categories, "categories");
        set_str_vec!(features, "features");

        if let Some(links_arr) = updates.get("links").and_then(|v| v.as_array()) {
            let links: Vec<crate::types::GameLink> = links_arr.iter().filter_map(|v| {
                let name = v.get("name")?.as_str()?.to_string();
                let url = v.get("url")?.as_str()?.to_string();
                Some(crate::types::GameLink { name, url })
            }).collect();
            if !links.is_empty() { game.links = Some(links); }
        }

        if let Some(actions_arr) = updates.get("gameActions").and_then(|v| v.as_array()) {
            let actions: Vec<crate::types::GameAction> = actions_arr.iter().filter_map(|v| {
                let name = v.get("name")?.as_str()?.to_string();
                let path = v.get("path")?.as_str()?.to_string();
                let arguments = v.get("arguments").and_then(|a| a.as_str()).map(|s| s.to_string());
                let working_dir = v.get("workingDir").and_then(|a| a.as_str()).map(|s| s.to_string());
                let is_default = v.get("isDefault").and_then(|a| a.as_bool()).unwrap_or(false);
                Some(crate::types::GameAction { name, path, arguments, working_dir, is_default })
            }).collect();
            if !actions.is_empty() { game.game_actions = Some(actions); }
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

    let client = get_http_client();

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
pub async fn download_steam_covers(app: AppHandle) -> Result<serde_json::Value, String> {
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
pub async fn download_game_cover(app: AppHandle, game_id: String) -> Result<Option<String>, String> {
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

// ==================== IGDB 元数据搜索 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn fetch_igdb_metadata(app: AppHandle, game_id: String, game_name: String) -> Result<serde_json::Value, String> {
    let client = get_http_client();

    let encoded = urlencoding::encode(&game_name);
    let search_url = format!(
        "https://api.igdb.com/v4/games/?search={}&fields=name,summary,genres.name,involved_companies.company.name,involved_companies.developer,involved_companies.publisher,first_release_date,total_rating,aggregated_rating,category&limit=1",
        encoded
    );

    let resp = client
        .post(&search_url)
        .header("Client-ID", "***IGDB_CLIENT_ID_REMOVED***")
        .header("Authorization", "Bearer ***IGDB_TOKEN_REMOVED***")
        .send()
        .await;

    let mut result = serde_json::json!({ "found": false });

    if let Ok(resp) = resp {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(games) = json.as_array() {
                if let Some(game_data) = games.first() {
                    let mut developers = Vec::new();
                    let mut publishers = Vec::new();

                    if let Some(companies) = game_data.get("involved_companies").and_then(|v| v.as_array()) {
                        for c in companies {
                            let name = c.get("company")
                                .and_then(|co| co.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            if name.is_empty() { continue; }
                            if c.get("developer").and_then(|d| d.as_bool()).unwrap_or(false) {
                                developers.push(name.to_string());
                            }
                            if c.get("publisher").and_then(|p| p.as_bool()).unwrap_or(false) {
                                publishers.push(name.to_string());
                            }
                        }
                    }

                    let mut genres = Vec::new();
                    if let Some(genre_arr) = game_data.get("genres").and_then(|v| v.as_array()) {
                        for g in genre_arr {
                            if let Some(name) = g.get("name").and_then(|n| n.as_str()) {
                                genres.push(name.to_string());
                            }
                        }
                    }

                    let description = game_data.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let critic_score = game_data.get("aggregated_rating").and_then(|v| v.as_f64()).map(|v| v as f32);
                    let community_score = game_data.get("total_rating").and_then(|v| v.as_f64()).map(|v| v as f32);

                    let mut release_year: Option<i32> = None;
                    let mut release_date: Option<String> = None;
                    if let Some(ts) = game_data.get("first_release_date").and_then(|v| v.as_i64()) {
                        let dt = chrono::DateTime::from_timestamp(ts, 0);
                        if let Some(dt) = dt {
                            release_year = Some(dt.format("%Y").to_string().parse().unwrap_or(0));
                            release_date = Some(dt.format("%Y-%m-%d").to_string());
                        }
                    }

                    let genre_str = if !genres.is_empty() { Some(genres.join(", ")) } else { None };

                    let mut updated = false;
                    if let Ok(mut all_games) = get_games(app.clone()).await {
                        if let Some(g) = all_games.iter_mut().find(|g| g.id == game_id) {
                            macro_rules! fill {
                                ($field:ident, $val:expr) => {
                                    if let Some(ref v) = $val {
                                        if g.$field.is_none() {
                                            g.$field = Some(v.clone());
                                            updated = true;
                                        }
                                    }
                                };
                            }
                            fill!(description, description);
                            fill!(genre, genre_str);
                            fill!(release_year, release_year);
                            fill!(release_date, release_date);
                            fill!(critic_score, critic_score);
                            fill!(community_score, community_score);
                            if !developers.is_empty() && g.developers.is_none() {
                                g.developers = Some(developers.clone());
                                updated = true;
                            }
                            if !publishers.is_empty() && g.publishers.is_none() {
                                g.publishers = Some(publishers.clone());
                                updated = true;
                            }
                        }
                        if updated {
                            let _ = save_games(app, all_games).await;
                        }
                    }

                    result = serde_json::json!({
                        "found": true,
                        "description": description,
                        "genre": genre_str,
                        "developers": developers,
                        "publishers": publishers,
                        "criticScore": critic_score,
                        "communityScore": community_score,
                        "releaseYear": release_year,
                        "releaseDate": release_date,
                        "updated": updated,
                    });
                }
            }
        }
    }

    Ok(result)
}

// ==================== SteamGridDB 封面搜索 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn search_cover_online(app: AppHandle, game_id: String, game_name: String) -> Result<Option<String>, String> {
    let client = get_http_client();

    let covers_dir = get_covers_dir(&app);
    let _ = std::fs::create_dir_all(&covers_dir);
    let dest_path = covers_dir.join(format!("{}.jpg", game_id));

    let encoded_name = urlencoding::encode(&game_name);
    let search_url = format!(
        "https://www.steamgriddb.com/api/v2/search/autocomplete/{}",
        encoded_name
    );

    if let Ok(resp) = client.get(&search_url)
        .header("Authorization", "Bearer ***STEAMGRIDDB_TOKEN_REMOVED***")
        .send().await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(first) = json.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) {
                if let Some(sgdb_id) = first.get("id").and_then(|v| v.as_i64()) {
                    let grids_url = format!(
                        "https://www.steamgriddb.com/api/v2/grids/game/{}?dimensions=600x900",
                        sgdb_id
                    );
                    if let Ok(grids_resp) = client.get(&grids_url)
                        .header("Authorization", "Bearer ***STEAMGRIDDB_TOKEN_REMOVED***")
                        .send().await
                    {
                        if let Ok(grids_json) = grids_resp.json::<serde_json::Value>().await {
                            if let Some(grid) = grids_json.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) {
                                if let Some(url) = grid.get("url").and_then(|v| v.as_str()) {
                                    if let Ok(img_resp) = client.get(url).send().await {
                                        if let Ok(bytes) = img_resp.bytes().await {
                                            if bytes.len() > 1000 {
                                                if std::fs::write(&dest_path, &bytes).is_ok() {
                                                    let cover_str = dest_path.to_string_lossy().to_string();
                                                    let mut all_games = get_games(app.clone()).await.unwrap_or_default();
                                                    if let Some(g) = all_games.iter_mut().find(|g| g.id == game_id) {
                                                        g.cover = Some(cover_str.clone());
                                                    }
                                                    let _ = save_games(app, all_games).await;
                                                    return Ok(Some(cover_str));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

// ==================== 备份与恢复 ====================

#[tauri::command]
pub async fn backup_library(app: AppHandle) -> Result<String, String> {
    use std::io::Write;

    let games_path = get_games_path(&app);
    let ignored_path = get_ignored_games_path(&app);
    let covers_dir = get_covers_dir(&app);

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_dir = games_path.parent().unwrap_or(Path::new(".")).join("backups");
    let _ = std::fs::create_dir_all(&backup_dir);
    let backup_file = backup_dir.join(format!("kogames_backup_{}.zip", timestamp));

    let file = std::fs::File::create(&backup_file).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    if games_path.exists() {
        let content = std::fs::read(&games_path).map_err(|e| e.to_string())?;
        zip.start_file("game_library.json", options).map_err(|e| e.to_string())?;
        zip.write_all(&content).map_err(|e| e.to_string())?;
    }

    if ignored_path.exists() {
        let content = std::fs::read(&ignored_path).map_err(|e| e.to_string())?;
        zip.start_file("ignored_games.json", options).map_err(|e| e.to_string())?;
        zip.write_all(&content).map_err(|e| e.to_string())?;
    }

    if covers_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&covers_dir) {
            for entry in entries.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if let Ok(content) = std::fs::read(entry.path()) {
                    let zip_path = format!("covers/{}", file_name);
                    let _ = zip.start_file(zip_path, options);
                    let _ = zip.write_all(&content);
                }
            }
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(backup_file.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn restore_backup(app: AppHandle, backup_path: String) -> Result<String, String> {
    use std::io::Read;

    let file = std::fs::File::open(&backup_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let games_path = get_games_path(&app);
    let ignored_path = get_ignored_games_path(&app);
    let covers_dir = get_covers_dir(&app);

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|e| e.to_string())?;

        if name == "game_library.json" {
            std::fs::write(&games_path, &content).map_err(|e| e.to_string())?;
        } else if name == "ignored_games.json" {
            std::fs::write(&ignored_path, &content).map_err(|e| e.to_string())?;
        } else if let Some(filename) = name.strip_prefix("covers/") {
            let _ = std::fs::create_dir_all(&covers_dir);
            let dest = covers_dir.join(filename);
            let _ = std::fs::write(&dest, &content);
        }
    }

    get_games_cache().store(Arc::new(None));
    Ok("Backup restored".to_string())
}

#[tauri::command]
pub async fn list_backups(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let games_path = get_games_path(&app);
    let backup_dir = games_path.parent().unwrap_or(Path::new(".")).join("backups");

    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let mut backups = vec![];
    if let Ok(entries) = std::fs::read_dir(&backup_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("kogames_backup_") && name.ends_with(".zip") {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                backups.push(serde_json::json!({
                    "name": name,
                    "path": entry.path().to_string_lossy(),
                    "size": size,
                }));
            }
        }
    }

    backups.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        nb.cmp(na)
    });

    Ok(backups)
}

// ==================== 随机游戏选择 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn pick_random_game(
    app: AppHandle,
    filter_platform: Option<String>,
    filter_status: Option<String>,
    exclude_hidden: Option<bool>,
) -> Result<Option<Game>, String> {
    let games = get_games(app).await.unwrap_or_default();

    let filtered: Vec<&Game> = games.iter().filter(|g| {
        if exclude_hidden.unwrap_or(true) && g.hidden.unwrap_or(false) {
            return false;
        }
        if let Some(ref platform) = filter_platform {
            if platform != "all" {
                if g.source.as_deref() != Some(platform.as_str()) {
                    return false;
                }
            }
        }
        if let Some(ref status) = filter_status {
            if status != "all" {
                if g.completion_status.as_deref() != Some(status.as_str()) {
                    return false;
                }
            }
        }
        true
    }).collect();

    if filtered.is_empty() {
        return Ok(None);
    }

    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as usize) % filtered.len();
    Ok(Some(filtered[idx].clone()))
}

// ==================== 库统计 ====================

#[tauri::command]
pub async fn get_library_stats(app: AppHandle) -> Result<serde_json::Value, String> {
    let games = get_games(app).await.unwrap_or_default();
    let visible: Vec<&Game> = games.iter().filter(|g| !g.hidden.unwrap_or(false)).collect();

    let total = visible.len();
    let total_playtime: i64 = visible.iter().map(|g| g.total_play_time.unwrap_or(0)).sum();
    let total_sessions: usize = visible.iter().map(|g| g.sessions.as_ref().map_or(0, |s| s.len())).sum();

    let mut by_platform: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut by_status: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut by_genre: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for g in &visible {
        let platform = g.source.as_deref().unwrap_or("unknown").to_string();
        *by_platform.entry(platform).or_insert(0) += 1;

        let status = g.completion_status.as_deref().unwrap_or("not_played").to_string();
        *by_status.entry(status).or_insert(0) += 1;

        if let Some(ref genre_str) = g.genre {
            for genre in genre_str.split(", ") {
                *by_genre.entry(genre.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut top_played: Vec<serde_json::Value> = visible.iter()
        .filter(|g| g.total_play_time.unwrap_or(0) > 0)
        .map(|g| serde_json::json!({
            "id": g.id,
            "name": g.name,
            "totalPlayTime": g.total_play_time.unwrap_or(0),
            "cover": g.cover,
        }))
        .collect();
    top_played.sort_by(|a, b| {
        let ta = a["totalPlayTime"].as_i64().unwrap_or(0);
        let tb = b["totalPlayTime"].as_i64().unwrap_or(0);
        tb.cmp(&ta)
    });
    top_played.truncate(20);

    Ok(serde_json::json!({
        "total": total,
        "totalPlayTimeMinutes": total_playtime,
        "totalSessions": total_sessions,
        "byPlatform": by_platform,
        "byStatus": by_status,
        "byGenre": by_genre,
        "topPlayed": top_played,
    }))
}

// ==================== 安装大小检测 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn detect_install_size(app: AppHandle, game_id: String) -> Result<Option<u64>, String> {
    let games = get_games(app.clone()).await.unwrap_or_default();
    let game = match games.iter().find(|g| g.id == game_id) {
        Some(g) => g.clone(),
        None => return Ok(None),
    };

    let install_loc = match &game.install_location {
        Some(loc) => loc.clone(),
        None => return Ok(None),
    };

    let size = tokio::task::spawn_blocking(move || {
        let path = std::path::Path::new(&install_loc);
        if !path.exists() { return 0u64; }
        dir_size(path)
    }).await.map_err(|e| e.to_string())?;

    if size > 0 {
        let mut all_games = get_games(app.clone()).await.unwrap_or_default();
        if let Some(g) = all_games.iter_mut().find(|g| g.id == game_id) {
            g.install_size = Some(size);
        }
        let _ = save_games(app, all_games).await;
    }

    Ok(if size > 0 { Some(size) } else { None })
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total += dir_size(&entry.path());
                } else {
                    total += meta.len();
                }
            }
        }
    }
    total
}

// ==================== 导入导出 ====================

#[tauri::command]
pub async fn export_library(app: AppHandle) -> Result<String, String> {
    let games = get_games(app.clone()).await.unwrap_or_default();
    let json = serde_json::to_string_pretty(&games).map_err(|e| e.to_string())?;

    let games_path = get_games_path(&app);
    let export_dir = games_path.parent().unwrap_or(Path::new("."));
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let export_file = export_dir.join(format!("kogames_export_{}.json", timestamp));

    std::fs::write(&export_file, &json).map_err(|e| e.to_string())?;
    Ok(export_file.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn import_library(app: AppHandle, file_path: String) -> Result<serde_json::Value, String> {
    let content = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let imported: Vec<Game> = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let existing = get_games(app.clone()).await.unwrap_or_default();
    let existing_ids: std::collections::HashSet<_> = existing.iter().map(|g| g.id.clone()).collect();

    let new_games: Vec<Game> = imported.into_iter()
        .filter(|g| !existing_ids.contains(&g.id))
        .collect();

    let new_count = new_games.len();

    if !new_games.is_empty() {
        let mut all = existing;
        all.extend(new_games);
        let _ = save_games(app, all).await;
    }

    Ok(serde_json::json!({
        "imported": new_count,
    }))
}

// ==================== 自动导入 ====================

#[tauri::command]
pub async fn auto_import_all(app: AppHandle) -> Result<serde_json::Value, String> {
    let mut total = 0;

    match crate::games::import_steam_games(app.clone()).await {
        Ok(games) => total += games.len(),
        Err(_) => {}
    }

    match crate::game_library::import_all_platform_games(app.clone()).await {
        serde_json::Value::Object(map) => {
            if let Some(t) = map.get("total").and_then(|v| v.as_i64()) {
                total += t as usize;
            }
        }
        _ => {}
    }

    Ok(serde_json::json!({
        "total": total,
    }))
}

// ==================== 批量元数据下载 ====================

#[tauri::command]
pub async fn batch_fetch_metadata(app: AppHandle) -> Result<serde_json::Value, String> {
    let games = get_games(app.clone()).await.unwrap_or_default();
    let needs_metadata: Vec<_> = games.iter()
        .filter(|g| g.description.is_none() || g.developers.is_none())
        .cloned()
        .collect();

    let total = needs_metadata.len();
    let mut updated = 0u32;

    // Process in chunks of 3 for rate limiting
    for chunk in needs_metadata.chunks(3) {
        let mut handles = Vec::new();
        for game in chunk {
            let app_c = app.clone();
            let id = game.id.clone();
            let name = game.name.clone();
            let steam_id = game.steam_app_id.clone();
            handles.push(tokio::spawn(async move {
                if let Some(app_id) = steam_id {
                    let _ = fetch_steam_metadata(app_c, id, app_id).await;
                } else {
                    let _ = fetch_igdb_metadata(app_c, id, name).await;
                }
            }));
        }
        for h in handles { let _ = h.await; }
        updated += chunk.len() as u32;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let games_for_covers = get_games(app.clone()).await.unwrap_or_default();
    let needs_cover: Vec<_> = games_for_covers.iter()
        .filter(|g| g.cover.is_none())
        .cloned()
        .collect();

    let covers_dir = get_covers_dir(&app);
    let _ = std::fs::create_dir_all(&covers_dir);
    let mut covers_downloaded = 0u32;

    for chunk in needs_cover.chunks(3) {
        let mut handles = Vec::new();
        for game in chunk {
            let app_c = app.clone();
            let gid = game.id.clone();
            let gname = game.name.clone();
            let sid = game.steam_app_id.clone();
            let cdir = covers_dir.clone();
            handles.push(tokio::spawn(async move {
                if let Some(app_id) = sid {
                    if let Some(p) = download_steam_cover_async(&app_id, &cdir, &gid).await {
                        let cover_str = p.to_string_lossy().to_string();
                        let mut all = get_games(app_c.clone()).await.unwrap_or_default();
                        if let Some(g) = all.iter_mut().find(|g| g.id == gid) {
                            g.cover = Some(cover_str);
                        }
                        let _ = save_games(app_c, all).await;
                        return true;
                    }
                } else {
                    let r = search_cover_online(app_c, gid, gname).await;
                    return r.map(|v| v.is_some()).unwrap_or(false);
                }
                false
            }));
        }
        for h in handles {
            if let Ok(true) = h.await { covers_downloaded += 1; }
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    Ok(serde_json::json!({
        "totalGames": total,
        "metadataUpdated": updated,
        "coversDownloaded": covers_downloaded,
    }))
}

// ==================== 筛选预设 ====================

#[tauri::command]
pub async fn save_filter_presets(app: AppHandle, presets: Vec<serde_json::Value>) -> Result<(), String> {
    let path = get_games_path(&app).with_file_name("filter_presets.json");
    let content = serde_json::to_string_pretty(&presets).map_err(|e| e.to_string())?;
    write_file_atomic(&path, &content)
}

#[tauri::command]
pub async fn load_filter_presets(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let path = get_games_path(&app).with_file_name("filter_presets.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let presets: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap_or_default();
    Ok(presets)
}

// ==================== 用户设置 ====================

#[tauri::command]
pub async fn save_user_settings(app: AppHandle, settings: serde_json::Value) -> Result<(), String> {
    let path = get_games_path(&app).with_file_name("settings.json");
    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    write_file_atomic(&path, &content)
}

#[tauri::command]
pub async fn load_user_settings(app: AppHandle) -> Result<serde_json::Value, String> {
    let path = get_games_path(&app).with_file_name("settings.json");
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let settings: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
    Ok(settings)
}

// ==================== 库健康检查 ====================

#[tauri::command]
pub async fn check_library_health(app: AppHandle) -> Result<serde_json::Value, String> {
    let games = get_games(app).await.unwrap_or_default();

    let mut missing_exe = Vec::new();
    let mut missing_install = Vec::new();
    let mut no_cover = 0u32;
    let mut no_metadata = 0u32;

    for game in &games {
        if let Some(ref path) = game.path {
            if !path.starts_with("shell:") && !path.starts_with("steam://") && !std::path::Path::new(path).exists() {
                missing_exe.push(serde_json::json!({ "id": game.id, "name": game.name, "path": path }));
            }
        }
        if let Some(ref loc) = game.install_location {
            if !std::path::Path::new(loc).exists() {
                missing_install.push(serde_json::json!({ "id": game.id, "name": game.name, "path": loc }));
            }
        }
        if game.cover.is_none() { no_cover += 1; }
        if game.description.is_none() && game.developers.is_none() { no_metadata += 1; }
    }

    Ok(serde_json::json!({
        "totalGames": games.len(),
        "missingExe": missing_exe,
        "missingInstall": missing_install,
        "noCover": no_cover,
        "noMetadata": no_metadata,
        "healthy": missing_exe.is_empty() && missing_install.is_empty(),
    }))
}

// ==================== 游戏时间线 ====================

#[tauri::command]
pub async fn get_play_timeline(app: AppHandle, days: Option<i32>) -> Result<serde_json::Value, String> {
    let days = days.unwrap_or(90);
    let games = get_games(app).await.unwrap_or_default();

    let now = chrono::Utc::now();
    let cutoff = now - chrono::Duration::days(days as i64);
    let cutoff_ms = cutoff.timestamp_millis();

    let mut daily: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut game_daily: std::collections::HashMap<String, std::collections::BTreeMap<String, i64>> = std::collections::HashMap::new();

    for game in &games {
        if let Some(ref sessions) = game.sessions {
            for session in sessions {
                if session.started_at < cutoff_ms { continue; }
                let dt = chrono::DateTime::from_timestamp_millis(session.started_at);
                if let Some(dt) = dt {
                    let day = dt.format("%Y-%m-%d").to_string();
                    *daily.entry(day.clone()).or_insert(0) += session.duration_minutes;
                    *game_daily.entry(game.name.clone()).or_default().entry(day).or_insert(0) += session.duration_minutes;
                }
            }
        }
    }

    let mut weekly: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for (day, mins) in &daily {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d") {
            let week = date.format("%Y-W%W").to_string();
            *weekly.entry(week).or_insert(0) += mins;
        }
    }

    let total_days_played = daily.len();
    let total_minutes: i64 = daily.values().sum();
    let max_day = daily.iter().max_by_key(|(_, v)| *v).map(|(k, v)| serde_json::json!({"date": k, "minutes": v}));

    Ok(serde_json::json!({
        "daily": daily,
        "weekly": weekly,
        "totalDaysPlayed": total_days_played,
        "totalMinutes": total_minutes,
        "maxDay": max_day,
    }))
}

// ==================== 后端排序/过滤/搜索 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn query_games(
    app: AppHandle,
    search: Option<String>,
    platform: Option<String>,
    status: Option<String>,
    tag: Option<String>,
    sort: Option<String>,
    show_hidden: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<serde_json::Value, String> {
    let all = get_games(app).await.unwrap_or_default();
    let show_hidden = show_hidden.unwrap_or(false);

    let mut filtered: Vec<&Game> = all.iter().filter(|g| {
        if !show_hidden && g.hidden.unwrap_or(false) { return false; }
        if let Some(ref p) = platform {
            if p != "all" && g.source.as_deref() != Some(p.as_str()) { return false; }
        }
        if let Some(ref s) = status {
            if s != "all" && g.completion_status.as_deref() != Some(s.as_str()) { return false; }
        }
        if let Some(ref t) = tag {
            if !g.tags.as_ref().map_or(false, |tags| tags.contains(t)) { return false; }
        }
        if let Some(ref q) = search {
            let q_lower = q.to_lowercase();
            let name_match = g.name.to_lowercase().contains(&q_lower);
            let dev_match = g.developers.as_ref().map_or(false, |d| d.iter().any(|x| x.to_lowercase().contains(&q_lower)));
            let pub_match = g.publishers.as_ref().map_or(false, |p| p.iter().any(|x| x.to_lowercase().contains(&q_lower)));
            let genre_match = g.genre.as_ref().map_or(false, |x| x.to_lowercase().contains(&q_lower));
            let tag_match = g.tags.as_ref().map_or(false, |t| t.iter().any(|x| x.to_lowercase().contains(&q_lower)));
            if !(name_match || dev_match || pub_match || genre_match || tag_match) { return false; }
        }
        true
    }).collect();

    match sort.as_deref() {
        Some("name") => filtered.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        Some("recent") => filtered.sort_by(|a, b| (b.last_played_at.unwrap_or(0)).cmp(&a.last_played_at.unwrap_or(0))),
        Some("most_played") => filtered.sort_by(|a, b| (b.total_play_time.unwrap_or(0)).cmp(&a.total_play_time.unwrap_or(0))),
        Some("added") => filtered.sort_by(|a, b| (b.added_at.unwrap_or(0)).cmp(&a.added_at.unwrap_or(0))),
        Some("platform") => filtered.sort_by(|a, b| a.source.as_deref().unwrap_or("zzz").cmp(b.source.as_deref().unwrap_or("zzz"))),
        Some("rating") => filtered.sort_by(|a, b| b.critic_score.unwrap_or(0.0).partial_cmp(&a.critic_score.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)),
        Some("user_rating") => filtered.sort_by(|a, b| b.user_rating.unwrap_or(0.0).partial_cmp(&a.user_rating.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)),
        Some("release") => filtered.sort_by(|a, b| (b.release_year.unwrap_or(0)).cmp(&a.release_year.unwrap_or(0))),
        _ => filtered.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
    }

    let total = filtered.len();
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(usize::MAX);

    let page: Vec<&Game> = filtered.into_iter().skip(offset).take(limit).collect();

    Ok(serde_json::json!({
        "total": total,
        "offset": offset,
        "count": page.len(),
        "games": page,
    }))
}

// ==================== 游戏日记 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn add_journal_entry(
    app: AppHandle,
    game_id: String,
    entry: serde_json::Value,
) -> Result<(), String> {
    let path = get_games_path(&app).with_file_name("game_journals.json");
    let mut journals: serde_json::Map<String, serde_json::Value> = if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    let entries = journals.entry(game_id).or_insert(serde_json::json!([]));
    if let Some(arr) = entries.as_array_mut() {
        let mut new_entry = entry;
        if let Some(obj) = new_entry.as_object_mut() {
            obj.insert("timestamp".to_string(), serde_json::json!(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64
            ));
        }
        arr.push(new_entry);
    }

    let content = serde_json::to_string_pretty(&journals).map_err(|e| e.to_string())?;
    write_file_atomic(&path, &content)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_journal_entries(
    app: AppHandle,
    game_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let path = get_games_path(&app).with_file_name("game_journals.json");
    if !path.exists() { return Ok(vec![]); }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let journals: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content).unwrap_or_default();

    let entries = journals.get(&game_id)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(entries)
}

// ==================== HowLongToBeat 集成 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn fetch_howlongtobeat(game_name: String) -> Result<serde_json::Value, String> {
    let client = get_http_client();

    let body = serde_json::json!({
        "searchType": "games",
        "searchTerms": game_name.split_whitespace().collect::<Vec<_>>(),
        "searchPage": 1,
        "size": 1,
        "searchOptions": {
            "games": { "userId": 0, "platform": "", "sortCategory": "popular", "rangeCategory": "main", "rangeTime": { "min": null, "max": null }, "gameplay": { "perspective": "", "flow": "", "genre": "" }, "rangeYear": { "min": "", "max": "" }, "modifier": "" },
            "users": { "sortCategory": "postcount" },
            "filter": "",
            "sort": 0,
            "randomizer": 0
        }
    });

    match client.post("https://howlongtobeat.com/api/search")
        .header("Content-Type", "application/json")
        .header("Referer", "https://howlongtobeat.com")
        .header("User-Agent", "KoGames/1.0")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) {
                    let main = data.get("comp_main").and_then(|v| v.as_f64()).map(|v| (v / 3600.0).round() as i64);
                    let extra = data.get("comp_plus").and_then(|v| v.as_f64()).map(|v| (v / 3600.0).round() as i64);
                    let completionist = data.get("comp_100").and_then(|v| v.as_f64()).map(|v| (v / 3600.0).round() as i64);
                    let game_name = data.get("game_name").and_then(|v| v.as_str()).unwrap_or("");

                    return Ok(serde_json::json!({
                        "found": true,
                        "name": game_name,
                        "mainStory": main,
                        "mainExtra": extra,
                        "completionist": completionist,
                    }));
                }
            }
        }
        Err(_) => {}
    }

    Ok(serde_json::json!({ "found": false }))
}

// ==================== 活动日志 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn log_activity(app: AppHandle, activity_type: String, details: String) -> Result<(), String> {
    let path = get_games_path(&app).with_file_name("activity_log.jsonl");

    let entry = serde_json::json!({
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
        "type": activity_type,
        "details": details,
    });

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{}", serde_json::to_string(&entry).unwrap_or_default()).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_recent_activity(app: AppHandle, limit: Option<usize>) -> Result<Vec<serde_json::Value>, String> {
    let path = get_games_path(&app).with_file_name("activity_log.jsonl");
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut entries: Vec<serde_json::Value> = content.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    entries.reverse();
    entries.truncate(limit.unwrap_or(50));

    Ok(entries)
}

// ==================== PCGamingWiki 集成 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn fetch_pcgw_info(game_name: String, steam_app_id: Option<String>) -> Result<serde_json::Value, String> {
    let client = get_http_client();

    let query = if let Some(ref app_id) = steam_app_id {
        format!(
            "https://www.pcgamingwiki.com/w/api.php?action=cargoquery&tables=Infobox_game&fields=Infobox_game._pageName=page,Infobox_game.Steam_AppID,Infobox_game.Developers,Infobox_game.Publishers,Infobox_game.Engines&where=Infobox_game.Steam_AppID%20HOLDS%20%22{}%22&format=json",
            app_id
        )
    } else {
        let encoded = urlencoding::encode(&game_name);
        format!(
            "https://www.pcgamingwiki.com/w/api.php?action=cargoquery&tables=Infobox_game&fields=Infobox_game._pageName=page,Infobox_game.Steam_AppID,Infobox_game.Developers,Infobox_game.Publishers,Infobox_game.Engines&where=Infobox_game._pageName%20LIKE%20%22{}%25%22&format=json&limit=1",
            encoded
        )
    };

    match client.get(&query).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(results) = json.get("cargoquery").and_then(|v| v.as_array()) {
                    if let Some(first) = results.first() {
                        if let Some(title) = first.get("title") {
                            let page = title.get("page").and_then(|v| v.as_str()).unwrap_or("");
                            let engine = title.get("Engines").and_then(|v| v.as_str()).unwrap_or("");
                            let wiki_url = format!("https://www.pcgamingwiki.com/wiki/{}", urlencoding::encode(page));
                            return Ok(serde_json::json!({
                                "found": true,
                                "page": page,
                                "engine": engine,
                                "wikiUrl": wiki_url,
                            }));
                        }
                    }
                }
            }
        }
        Err(_) => {}
    }

    Ok(serde_json::json!({ "found": false }))
}

// ==================== 游戏截图管理 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn find_screenshots(game_name: String, steam_app_id: Option<String>) -> Result<Vec<serde_json::Value>, String> {
    let mut results = Vec::new();

    if let Some(ref app_id) = steam_app_id {
        if let Some(steam_path) = crate::paths::get_steam_install_path() {
            let screenshots_dir = steam_path.join("userdata");
            if screenshots_dir.exists() {
                if let Ok(users) = std::fs::read_dir(&screenshots_dir) {
                    for user in users.flatten() {
                        let ss_dir = user.path().join("760").join("remote").join(app_id).join("screenshots");
                        if ss_dir.exists() {
                            if let Ok(files) = std::fs::read_dir(&ss_dir) {
                                for file in files.flatten() {
                                    let name = file.file_name().to_string_lossy().to_string();
                                    if name.ends_with(".jpg") || name.ends_with(".png") {
                                        results.push(serde_json::json!({
                                            "path": file.path().to_string_lossy(),
                                            "name": name,
                                            "source": "steam",
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let video_dir = PathBuf::from(&home).join("Videos").join("Captures");
    if video_dir.exists() {
        let clean_name = game_name.to_lowercase();
        if let Ok(files) = std::fs::read_dir(&video_dir) {
            for file in files.flatten() {
                let fname = file.file_name().to_string_lossy().to_lowercase();
                if fname.contains(&clean_name) && (fname.ends_with(".png") || fname.ends_with(".jpg") || fname.ends_with(".mp4")) {
                    results.push(serde_json::json!({
                        "path": file.path().to_string_lossy(),
                        "name": file.file_name().to_string_lossy(),
                        "source": "windows_capture",
                    }));
                }
            }
        }
    }

    results.truncate(50);
    Ok(results)
}

// ==================== Special K 集成 ====================

#[tauri::command]
pub async fn detect_special_k() -> Result<serde_json::Value, String> {
    let mut sk_path: Option<String> = None;
    let mut sk_version: Option<String> = None;

    let search_paths = [
        std::env::var("PROGRAMFILES").unwrap_or_default() + r"\Special K",
        std::env::var("PROGRAMFILES(X86)").unwrap_or_default() + r"\Special K",
        std::env::var("LOCALAPPDATA").unwrap_or_default() + r"\Programs\Special K",
        std::env::var("USERPROFILE").unwrap_or_default() + r"\Special K",
    ];

    for path in &search_paths {
        let dll_32 = PathBuf::from(path).join("SpecialK32.dll");
        let dll_64 = PathBuf::from(path).join("SpecialK64.dll");
        let skif = PathBuf::from(path).join("SKIF.exe");

        if dll_64.exists() || dll_32.exists() || skif.exists() {
            sk_path = Some(path.clone());
            let ver_file = PathBuf::from(path).join("Version").join("installed.ini");
            if ver_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&ver_file) {
                    for line in content.lines() {
                        if line.starts_with("Version=") {
                            sk_version = Some(line.trim_start_matches("Version=").trim().to_string());
                            break;
                        }
                    }
                }
            }
            break;
        }
    }

    Ok(serde_json::json!({
        "installed": sk_path.is_some(),
        "path": sk_path,
        "version": sk_version,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn launch_with_special_k(
    app: AppHandle,
    game_id: String,
    game_path: String,
) -> CommandResult {
    let sk_info = detect_special_k().await.unwrap_or(serde_json::json!({"installed": false}));
    if !sk_info.get("installed").and_then(|v| v.as_bool()).unwrap_or(false) {
        return CommandResult::error("Special K not installed");
    }

    let sk_path = sk_info.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let skif_exe = PathBuf::from(sk_path).join("SKIF.exe");

    if skif_exe.exists() {
        #[cfg(windows)]
        {
            let _ = Command::new(&skif_exe)
                .arg(&game_path)
                .creation_flags(0x08000000)
                .spawn();
        }
    } else {
        #[cfg(windows)]
        {
            let _ = Command::new(&game_path)
                .creation_flags(0x08000000)
                .spawn();
        }
    }

    if let Ok(mut games) = get_games(app.clone()).await {
        if let Some(game) = games.iter_mut().find(|g| g.id == game_id) {
            game.last_played_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            );
            let _ = save_games(app, games).await;
        }
    }

    CommandResult::success("Game launched with Special K")
}

// ==================== 游戏启动参数管理 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn save_launch_config(
    app: AppHandle,
    game_id: String,
    config: serde_json::Value,
) -> Result<(), String> {
    let path = get_games_path(&app).with_file_name("launch_configs.json");
    let mut configs: serde_json::Map<String, serde_json::Value> = if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    configs.insert(game_id, config);
    let content = serde_json::to_string_pretty(&configs).map_err(|e| e.to_string())?;
    write_file_atomic(&path, &content)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn load_launch_config(
    app: AppHandle,
    game_id: String,
) -> Result<serde_json::Value, String> {
    let path = get_games_path(&app).with_file_name("launch_configs.json");
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let configs: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content).unwrap_or_default();
    Ok(configs.get(&game_id).cloned().unwrap_or(serde_json::json!({})))
}

// ==================== 游戏推荐 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn get_recommendations(app: AppHandle, game_id: String) -> Result<Vec<serde_json::Value>, String> {
    let games = get_games(app).await.unwrap_or_default();
    let target = match games.iter().find(|g| g.id == game_id) {
        Some(g) => g,
        None => return Ok(vec![]),
    };

    let target_genres: Vec<&str> = target.genre.as_deref()
        .map(|g| g.split(", ").collect())
        .unwrap_or_default();
    let target_devs: Vec<&str> = target.developers.as_ref()
        .map(|d| d.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    let mut scored: Vec<(f32, &Game)> = games.iter()
        .filter(|g| g.id != game_id && !g.hidden.unwrap_or(false))
        .map(|g| {
            let mut score = 0.0f32;
            if let Some(ref genre) = g.genre {
                for tg in &target_genres {
                    if genre.contains(tg) { score += 2.0; }
                }
            }
            if let Some(ref devs) = g.developers {
                for td in &target_devs {
                    if devs.iter().any(|d| d == td) { score += 3.0; }
                }
            }
            if g.series.is_some() && g.series == target.series { score += 5.0; }
            if let Some(ref tags) = g.tags {
                if let Some(ref ttags) = target.tags {
                    for t in tags {
                        if ttags.contains(t) { score += 1.0; }
                    }
                }
            }
            (score, g)
        })
        .filter(|(s, _)| *s > 0.0)
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let results: Vec<serde_json::Value> = scored.iter().take(10).map(|(score, g)| {
        serde_json::json!({
            "id": g.id,
            "name": g.name,
            "cover": g.cover,
            "genre": g.genre,
            "score": score,
        })
    }).collect();

    Ok(results)
}

// ==================== 重复游戏检测 ====================

#[tauri::command]
pub async fn detect_duplicates(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let games = get_games(app).await.unwrap_or_default();
    let mut name_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for g in &games {
        let key = g.name.to_lowercase().trim().to_string();
        name_map.entry(key).or_default().push(g.id.clone());
    }

    let duplicates: Vec<serde_json::Value> = name_map.iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(name, ids)| {
            serde_json::json!({
                "name": name,
                "count": ids.len(),
                "ids": ids,
            })
        })
        .collect();

    Ok(duplicates)
}

// ==================== URL 启动 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn launch_url(url: String) -> CommandResult {
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
    CommandResult::success("URL launched")
}

// ==================== 年度回顾 ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn get_year_in_review(app: AppHandle, year: Option<i32>) -> Result<serde_json::Value, String> {
    let year = year.unwrap_or_else(|| chrono::Local::now().format("%Y").to_string().parse().unwrap_or(2026));
    let games = get_games(app).await.unwrap_or_default();

    let year_start = chrono::NaiveDate::from_ymd_opt(year, 1, 1)
        .unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
    let year_end = chrono::NaiveDate::from_ymd_opt(year, 12, 31)
        .unwrap().and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp_millis();

    let mut total_minutes = 0i64;
    let mut total_sessions = 0usize;
    let mut game_times: Vec<(String, String, i64, Option<String>)> = Vec::new();
    let mut monthly: [i64; 12] = [0; 12];
    let mut games_added = 0usize;
    let mut games_completed = 0usize;
    let mut new_games_played = 0usize;
    let mut unique_days = std::collections::HashSet::new();

    for game in &games {
        let mut game_year_time = 0i64;

        if let Some(ref sessions) = game.sessions {
            for s in sessions {
                if s.started_at >= year_start && s.started_at <= year_end {
                    game_year_time += s.duration_minutes;
                    total_sessions += 1;

                    if let Some(dt) = chrono::DateTime::from_timestamp_millis(s.started_at) {
                        let month = dt.format("%m").to_string().parse::<usize>().unwrap_or(1) - 1;
                        monthly[month] += s.duration_minutes;
                        unique_days.insert(dt.format("%Y-%m-%d").to_string());
                    }
                }
            }
        }

        if game_year_time > 0 {
            game_times.push((game.id.clone(), game.name.clone(), game_year_time, game.cover.clone()));
            new_games_played += 1;
        }
        total_minutes += game_year_time;

        if let Some(added) = game.added_at {
            if added >= year_start && added <= year_end {
                games_added += 1;
            }
        }

        if game.completion_status.as_deref() == Some("completed") {
            if let Some(ref sessions) = game.sessions {
                if sessions.iter().any(|s| s.ended_at >= year_start && s.ended_at <= year_end) {
                    games_completed += 1;
                }
            }
        }
    }

    game_times.sort_by(|a, b| b.2.cmp(&a.2));
    let top_games: Vec<serde_json::Value> = game_times.iter().take(10).map(|(id, name, mins, cover)| {
        serde_json::json!({ "id": id, "name": name, "minutes": mins, "cover": cover })
    }).collect();

    let monthly_labels = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
    let monthly_data: Vec<serde_json::Value> = monthly.iter().enumerate().map(|(i, mins)| {
        serde_json::json!({ "month": monthly_labels[i], "minutes": mins })
    }).collect();

    Ok(serde_json::json!({
        "year": year,
        "totalMinutes": total_minutes,
        "totalHours": total_minutes / 60,
        "totalSessions": total_sessions,
        "uniqueDaysPlayed": unique_days.len(),
        "gamesPlayed": new_games_played,
        "gamesAdded": games_added,
        "gamesCompleted": games_completed,
        "topGames": top_games,
        "monthly": monthly_data,
    }))
}

// ==================== 游戏成就里程碑 ====================

#[tauri::command]
pub async fn get_milestones(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let games = get_games(app).await.unwrap_or_default();
    let mut milestones = Vec::new();

    let total = games.len();
    let total_hours: i64 = games.iter().map(|g| g.total_play_time.unwrap_or(0)).sum::<i64>() / 60;
    let completed = games.iter().filter(|g| g.completion_status.as_deref() == Some("completed")).count();
    let total_sessions: usize = games.iter().map(|g| g.sessions.as_ref().map_or(0, |s| s.len())).sum();
    let platforms: std::collections::HashSet<_> = games.iter().filter_map(|g| g.source.as_deref()).collect();

    let checks: Vec<(&str, &str, bool)> = vec![
        ("first_game", "First game added!", total >= 1),
        ("ten_games", "10 games in library", total >= 10),
        ("fifty_games", "50 games in library", total >= 50),
        ("hundred_games", "100 games in library!", total >= 100),
        ("first_hour", "First hour of gaming", total_hours >= 1),
        ("ten_hours", "10 hours played", total_hours >= 10),
        ("hundred_hours", "100 hours played!", total_hours >= 100),
        ("thousand_hours", "1000 hours played!!!", total_hours >= 1000),
        ("first_completion", "First game completed", completed >= 1),
        ("five_completions", "5 games completed", completed >= 5),
        ("ten_completions", "10 games completed!", completed >= 10),
        ("multi_platform", "Games from 3+ platforms", platforms.len() >= 3),
        ("all_platforms", "Games from 5+ platforms!", platforms.len() >= 5),
        ("session_veteran", "100 play sessions", total_sessions >= 100),
        ("session_master", "500 play sessions!", total_sessions >= 500),
    ];

    for (id, desc, achieved) in checks {
        milestones.push(serde_json::json!({
            "id": id,
            "description": desc,
            "achieved": achieved,
        }));
    }

    Ok(milestones)
}
