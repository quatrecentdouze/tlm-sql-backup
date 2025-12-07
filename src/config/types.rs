use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseEngine {
    MySQL,
}

impl std::fmt::Display for DatabaseEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseEngine::MySQL => write!(f, "MySQL"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub name: String,
    pub engine: DatabaseEngine,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            engine: DatabaseEngine::MySQL,
            host: "localhost".to_string(),
            port: 3306,
            username: "root".to_string(),
            password: String::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Schedule {
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

impl Schedule {
    pub fn as_seconds(&self) -> u64 {
        match self {
            Schedule::Minutes(n) => *n as u64 * 60,
            Schedule::Hours(n) => *n as u64 * 3600,
            Schedule::Days(n) => *n as u64 * 86400,
        }
    }
}

impl std::fmt::Display for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Schedule::Minutes(n) => write!(f, "Every {} minute(s)", n),
            Schedule::Hours(n) => write!(f, "Every {} hour(s)", n),
            Schedule::Days(n) => write!(f, "Every {} day(s)", n),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupJob {
    pub db_config_name: String,
    pub databases: Vec<String>,
    pub schedule: Schedule,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub bot_token: String,
    pub guild_id: u64,
    pub forum_channel_name: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UploadConfig {
    pub discord: Option<DiscordConfig>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    pub enabled: bool,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8080,
            username: String::new(),
            password: String::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub databases: Vec<DatabaseConfig>,
    #[serde(default)]
    pub backup_jobs: Vec<BackupJob>,
    #[serde(default)]
    pub upload: UploadConfig,
    #[serde(default)]
    pub web: WebConfig,
    pub local_backup_dir: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            databases: Vec::new(),
            backup_jobs: Vec::new(),
            upload: UploadConfig::default(),
            web: WebConfig::default(),
            local_backup_dir: PathBuf::from("backups"),
        }
    }
}
