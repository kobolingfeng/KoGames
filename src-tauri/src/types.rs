//! 类型定义

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub path: Option<String>,

    #[serde(rename = "steamAppId", alias = "steam_app_id")]
    pub steam_app_id: Option<String>,

    pub source: Option<String>,
    pub cover: Option<String>,

    #[serde(rename = "addedAt", alias = "added_at")]
    pub added_at: Option<i64>,

    #[serde(rename = "lastPlayedAt", alias = "last_played_at")]
    pub last_played_at: Option<i64>,

    #[serde(rename = "installLocation", alias = "install_location")]
    pub install_location: Option<String>,

    #[serde(default)]
    pub pinned: Option<bool>,

    #[serde(rename = "completionStatus", alias = "completion_status")]
    pub completion_status: Option<String>,

    #[serde(rename = "totalPlayTime", alias = "total_play_time")]
    pub total_play_time: Option<i64>,

    pub description: Option<String>,
    pub genre: Option<String>,

    #[serde(rename = "releaseYear", alias = "release_year")]
    pub release_year: Option<i32>,

    #[serde(default)]
    pub favorite: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl CommandResult {
    pub fn success(message: impl Into<String>) -> Self {
        CommandResult {
            success: true,
            message: Some(message.into()),
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        CommandResult {
            success: false,
            message: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub has_battery: bool,
    pub is_charging: bool,
    pub is_ac_connected: bool,
    pub battery_percent: i32,
    pub battery_life_time: Option<i32>,
    pub power_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverResult {
    pub game_id: String,
    pub cover_path: Option<String>,
}
