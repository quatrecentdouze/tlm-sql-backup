mod types;

pub use types::*;

use crate::error::{BackupError, Result};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".db_backup_cli"))
        .unwrap_or_else(|| PathBuf::from(".db_backup_cli"))
}
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}
pub fn load() -> Result<AppConfig> {
    load_from(&config_path())
}
pub fn load_from(path: &PathBuf) -> Result<AppConfig> {
    if !path.exists() {
        debug!("Config file not found at {:?}, using defaults", path);
        return Ok(AppConfig::default());
    }

    info!("Loading configuration from {:?}", path);
    let contents = fs::read_to_string(path)?;
    let config: AppConfig = toml::from_str(&contents)?;
    Ok(config)
}
pub fn save(config: &AppConfig) -> Result<()> {
    save_to(config, &config_path())
}
pub fn save_to(config: &AppConfig, path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            info!("Creating config directory: {:?}", parent);
            fs::create_dir_all(parent)?;
        }
    }

    let contents = toml::to_string_pretty(config)
        .map_err(|e| BackupError::Serialization(e.to_string()))?;
    
    fs::write(path, contents)?;
    info!("Configuration saved to {:?}", path);
    Ok(())
}
pub fn exists() -> bool {
    config_path().exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let config = AppConfig {
            databases: vec![DatabaseConfig {
                name: "test".to_string(),
                engine: DatabaseEngine::MySQL,
                host: "localhost".to_string(),
                port: 3306,
                username: "root".to_string(),
                password: "secret".to_string(),
            }],
            backup_jobs: vec![BackupJob {
                db_config_name: "test".to_string(),
                databases: vec!["mydb".to_string()],
                schedule: Schedule::Hours(1),
            }],
            upload: UploadConfig {
                discord: Some(DiscordConfig {
                    bot_token: "token".to_string(),
                    guild_id: 123456789,
                    forum_channel_name: "backups".to_string(),
                }),
            },
            local_backup_dir: PathBuf::from("backups"),
        };

        save_to(&config, &path).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.databases.len(), 1);
        assert_eq!(loaded.databases[0].name, "test");
        assert_eq!(loaded.backup_jobs.len(), 1);
        assert!(loaded.upload.discord.is_some());
    }

    #[test]
    fn test_schedule_as_seconds() {
        assert_eq!(Schedule::Minutes(5).as_seconds(), 300);
        assert_eq!(Schedule::Hours(2).as_seconds(), 7200);
        assert_eq!(Schedule::Days(1).as_seconds(), 86400);
    }
}
