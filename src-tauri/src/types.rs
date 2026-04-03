//! 类型定义

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameAction {
    pub name: String,
    pub path: String,
    pub arguments: Option<String>,
    #[serde(rename = "workingDir", alias = "working_dir")]
    pub working_dir: Option<String>,
    #[serde(rename = "isDefault", alias = "is_default", default)]
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameLink {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameSession {
    #[serde(rename = "startedAt", alias = "started_at")]
    pub started_at: i64,
    #[serde(rename = "endedAt", alias = "ended_at")]
    pub ended_at: i64,
    #[serde(rename = "durationMinutes", alias = "duration_minutes")]
    pub duration_minutes: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub path: Option<String>,

    #[serde(rename = "steamAppId", alias = "steam_app_id")]
    pub steam_app_id: Option<String>,

    pub source: Option<String>,
    pub cover: Option<String>,

    #[serde(rename = "backgroundImage", alias = "background_image")]
    pub background_image: Option<String>,

    pub icon: Option<String>,
    pub logo: Option<String>,

    #[serde(rename = "addedAt", alias = "added_at")]
    pub added_at: Option<i64>,

    #[serde(rename = "lastPlayedAt", alias = "last_played_at")]
    pub last_played_at: Option<i64>,

    #[serde(rename = "installLocation", alias = "install_location")]
    pub install_location: Option<String>,

    #[serde(rename = "installSize", alias = "install_size")]
    pub install_size: Option<u64>,

    #[serde(default)]
    pub pinned: Option<bool>,

    #[serde(rename = "completionStatus", alias = "completion_status")]
    pub completion_status: Option<String>,

    #[serde(rename = "totalPlayTime", alias = "total_play_time")]
    pub total_play_time: Option<i64>,

    #[serde(rename = "playCount", alias = "play_count")]
    pub play_count: Option<i32>,

    pub description: Option<String>,
    pub genre: Option<String>,

    #[serde(rename = "releaseYear", alias = "release_year")]
    pub release_year: Option<i32>,

    #[serde(rename = "releaseDate", alias = "release_date")]
    pub release_date: Option<String>,

    #[serde(default)]
    pub favorite: Option<bool>,

    #[serde(default)]
    pub hidden: Option<bool>,

    pub developers: Option<Vec<String>>,
    pub publishers: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub features: Option<Vec<String>>,
    pub series: Option<String>,

    #[serde(rename = "ageRating", alias = "age_rating")]
    pub age_rating: Option<String>,

    #[serde(rename = "criticScore", alias = "critic_score")]
    pub critic_score: Option<f32>,

    #[serde(rename = "communityScore", alias = "community_score")]
    pub community_score: Option<f32>,

    pub notes: Option<String>,

    pub links: Option<Vec<GameLink>>,

    #[serde(rename = "gameActions", alias = "game_actions")]
    pub game_actions: Option<Vec<GameAction>>,

    #[serde(rename = "preScript", alias = "pre_script")]
    pub pre_script: Option<String>,

    #[serde(rename = "postScript", alias = "post_script")]
    pub post_script: Option<String>,

    pub platform: Option<String>,

    pub sessions: Option<Vec<GameSession>>,

    #[serde(rename = "userRating", alias = "user_rating")]
    pub user_rating: Option<f32>,

    #[serde(rename = "purchasePrice", alias = "purchase_price")]
    pub purchase_price: Option<f64>,

    #[serde(rename = "purchaseDate", alias = "purchase_date")]
    pub purchase_date: Option<String>,

    #[serde(rename = "purchaseStore", alias = "purchase_store")]
    pub purchase_store: Option<String>,
}

impl Game {
    pub fn new_import(id: String, name: String, source: &str, path: Option<String>, install_location: Option<String>) -> Self {
        Game {
            id,
            name,
            path,
            steam_app_id: None,
            source: Some(source.to_string()),
            cover: None,
            background_image: None,
            icon: None,
            logo: None,
            added_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            ),
            last_played_at: None,
            install_location,
            install_size: None,
            pinned: None,
            completion_status: None,
            total_play_time: None,
            play_count: None,
            description: None,
            genre: None,
            release_year: None,
            release_date: None,
            favorite: None,
            hidden: None,
            developers: None,
            publishers: None,
            tags: None,
            categories: None,
            features: None,
            series: None,
            age_rating: None,
            critic_score: None,
            community_score: None,
            notes: None,
            links: None,
            game_actions: None,
            pre_script: None,
            post_script: None,
            platform: None,
            sessions: None,
            user_rating: None,
            purchase_price: None,
            purchase_date: None,
            purchase_store: None,
        }
    }
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
