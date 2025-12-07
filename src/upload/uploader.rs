use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::Path;
#[derive(Debug, Clone)]
pub struct BackupMetadata {
    pub databases: Vec<String>,
    pub connection_name: String,
    pub timestamp: DateTime<Utc>,
    pub file_size: u64,
    pub file_hash: Option<String>,
    pub duration_secs: u64,
    pub file_path: String,
}
#[async_trait]
pub trait BackupUploader: Send + Sync {
    async fn upload(&self, metadata: &BackupMetadata, file_path: &Path) -> Result<()>;
    async fn upload_silent(&self, metadata: &BackupMetadata, file_path: &Path, silent: bool) -> Result<()>;
    async fn test_connection(&self) -> Result<()>;
    fn name(&self) -> &'static str;
}
