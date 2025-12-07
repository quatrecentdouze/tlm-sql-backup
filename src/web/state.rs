use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerStatus {

    pub running: bool,

    pub next_run: Option<DateTime<Utc>>,

    pub interval_secs: u64,

    pub connection_name: Option<String>,

    pub database_count: usize,
}

impl Default for SchedulerStatus {
    fn default() -> Self {
        Self {
            running: false,
            next_run: None,
            interval_secs: 0,
            connection_name: None,
            database_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupEntry {

    pub timestamp: DateTime<Utc>,

    pub connection_name: String,

    pub databases: Vec<String>,

    pub success: bool,

    pub file_size: u64,

    pub duration_secs: u64,

    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub message: String,
}

pub struct AppState {

    pub scheduler: RwLock<SchedulerStatus>,

    pub history: RwLock<Vec<BackupEntry>>,

    pub config_summary: RwLock<ConfigSummary>,

    credentials: RwLock<(String, String)>,

    pub scheduler_logs: RwLock<Vec<LogEntry>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ConfigSummary {
    pub database_connections: usize,
    pub backup_jobs: usize,
    pub discord_configured: bool,
    pub backup_directory: String,
}

impl AppState {

    pub fn new(username: String, password: String) -> Arc<Self> {
        Arc::new(Self {
            scheduler: RwLock::new(SchedulerStatus::default()),
            history: RwLock::new(Vec::new()),
            config_summary: RwLock::new(ConfigSummary::default()),
            credentials: RwLock::new((username, password)),
            scheduler_logs: RwLock::new(Vec::new()),
        })
    }

    pub async fn set_credentials(&self, username: String, password: String) {
        let mut creds = self.credentials.write().await;
        *creds = (username, password);
    }

    pub async fn check_credentials(&self, username: &str, password: &str) -> bool {
        let creds = self.credentials.read().await;
        creds.0 == username && creds.1 == password
    }

    pub async fn update_scheduler(&self, status: SchedulerStatus) {
        let mut scheduler = self.scheduler.write().await;
        *scheduler = status;
    }

    pub async fn add_backup_entry(&self, entry: BackupEntry) {
        let mut history = self.history.write().await;
        history.insert(0, entry);
        if history.len() > 50 {
            history.truncate(50);
        }
    }

    pub async fn update_config(&self, summary: ConfigSummary) {
        let mut config = self.config_summary.write().await;
        *config = summary;
    }

    pub async fn add_log(&self, level: &str, message: &str) {
        let mut logs = self.scheduler_logs.write().await;
        logs.insert(0, LogEntry {
            timestamp: Utc::now(),
            level: level.to_string(),
            message: message.to_string(),
        });
        if logs.len() > 100 {
            logs.truncate(100);
        }
    }

    pub async fn clear_logs(&self) {
        let mut logs = self.scheduler_logs.write().await;
        logs.clear();
    }
}
