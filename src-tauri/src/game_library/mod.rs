//! 多平台游戏库扫描模块

pub mod battlenet;
pub mod ea;
pub mod epic;
pub mod gog;
pub mod ubisoft;
pub mod xbox;

use crate::games::{get_games, load_ignored_game_ids, save_games};
use crate::types::Game;
use tauri::AppHandle;

#[tauri::command]
pub async fn import_platform_games(app: AppHandle, platform: String) -> Result<Vec<Game>, String> {
    let platform_clone = platform.clone();
    let games = tokio::task::spawn_blocking(move || -> Result<Vec<Game>, String> {
        match platform_clone.as_str() {
            "epic" => epic::scan_epic_games(),
            "ea" => ea::scan_ea_games(),
            "ubisoft" => ubisoft::scan_ubisoft_games(),
            "xbox" => xbox::scan_xbox_games(),
            "gog" => gog::scan_gog_games(),
            "battlenet" => battlenet::scan_battlenet_games(),
            _ => Err(format!("Unsupported platform: {}", platform_clone)),
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    if games.is_empty() {
        return Ok(vec![]);
    }

    let ignored_ids = load_ignored_game_ids(&app);
    let existing = get_games(app.clone()).await.unwrap_or_default();
    let existing_ids: std::collections::HashSet<_> =
        existing.iter().map(|g| g.id.clone()).collect();

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

#[tauri::command]
pub async fn import_all_platform_games(app: AppHandle) -> serde_json::Value {
    let mut results = serde_json::json!({});
    let mut total_imported = 0;

    for platform in ["epic", "ea", "ubisoft", "xbox", "gog", "battlenet"] {
        match import_platform_games(app.clone(), platform.to_string()).await {
            Ok(games) => {
                let count = games.len();
                total_imported += count;
                results[platform] = serde_json::json!({ "success": true, "count": count });
            }
            Err(e) => {
                results[platform] = serde_json::json!({ "success": false, "error": e });
            }
        }
    }

    results["total"] = serde_json::json!(total_imported);
    results
}

#[tauri::command]
pub async fn detect_installed_platforms() -> Vec<serde_json::Value> {
    tokio::task::spawn_blocking(|| {
        let mut platforms = vec![];

        if epic::is_epic_installed() {
            platforms.push(serde_json::json!({ "id": "epic", "name": "Epic Games", "installed": true }));
        }
        if ea::is_ea_installed() {
            platforms.push(serde_json::json!({ "id": "ea", "name": "EA App", "installed": true }));
        }
        if ubisoft::is_ubisoft_installed() {
            platforms.push(serde_json::json!({ "id": "ubisoft", "name": "Ubisoft Connect", "installed": true }));
        }
        if xbox::is_xbox_installed() {
            platforms.push(serde_json::json!({ "id": "xbox", "name": "Xbox/Microsoft Store", "installed": true }));
        }
        if gog::is_gog_installed() {
            platforms.push(serde_json::json!({ "id": "gog", "name": "GOG Galaxy", "installed": true }));
        }
        if battlenet::is_battlenet_installed() {
            platforms.push(serde_json::json!({ "id": "battlenet", "name": "Battle.net", "installed": true }));
        }

        platforms
    })
    .await
    .unwrap_or_default()
}
