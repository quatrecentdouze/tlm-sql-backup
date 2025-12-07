use crate::backup::compression::{calculate_sha256, compress_multiple_to_zip_silent};
use crate::config::{AppConfig, DatabaseConfig};
use crate::database::create_driver;
use crate::error::Result;
use crate::upload::{create_uploaders, BackupMetadata};
use chrono::Utc;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{error, info, warn};

#[derive(Debug)]
pub struct BackupResult {

    pub connection_name: String,

    pub databases: Vec<String>,

    pub success: bool,

    pub file_path: Option<PathBuf>,

    pub file_size: Option<u64>,

    pub duration_secs: u64,

    pub error: Option<String>,

    pub db_errors: Vec<(String, String)>,
}

pub async fn execute_job_backup(
    config: &AppConfig,
    db_config: &DatabaseConfig,
    databases: &[String],
) -> BackupResult {
    execute_job_backup_internal(config, db_config, databases, false).await
}

pub async fn execute_job_backup_silent(
    config: &AppConfig,
    db_config: &DatabaseConfig,
    databases: &[String],
) -> BackupResult {
    execute_job_backup_internal(config, db_config, databases, true).await
}

async fn execute_job_backup_internal(
    config: &AppConfig,
    db_config: &DatabaseConfig,
    databases: &[String],
    silent: bool,
) -> BackupResult {
    let start = Instant::now();
    let timestamp = Utc::now();
    let timestamp_str = timestamp.format("%Y%m%d_%H%M%S").to_string();
    
    if !silent {
        info!(
            "Starting combined backup for {} databases on connection '{}'",
            databases.len(),
            db_config.name
        );
    }
    let backup_dir = config.local_backup_dir.join(&db_config.name);
    
    if let Err(e) = fs::create_dir_all(&backup_dir) {
        return BackupResult {
            connection_name: db_config.name.clone(),
            databases: databases.to_vec(),
            success: false,
            file_path: None,
            file_size: None,
            duration_secs: start.elapsed().as_secs(),
            error: Some(format!("Failed to create backup directory: {}", e)),
            db_errors: vec![],
        };
    }
    let driver = match create_driver(db_config) {
        Ok(d) => d,
        Err(e) => {
            return BackupResult {
                connection_name: db_config.name.clone(),
                databases: databases.to_vec(),
                success: false,
                file_path: None,
                file_size: None,
                duration_secs: start.elapsed().as_secs(),
                error: Some(format!("Failed to create database driver: {}", e)),
                db_errors: vec![],
            };
        }
    };
    let mut sql_files: Vec<(PathBuf, String)> = Vec::new();
    let mut db_errors: Vec<(String, String)> = Vec::new();
    let mut successful_dbs: Vec<String> = Vec::new();

    for db_name in databases {
        if !silent {
            info!("Dumping database: {}", db_name);
        }
        
        let sql_filename = format!("{}_{}.sql", db_name, timestamp_str);
        let sql_path = backup_dir.join(&sql_filename);
        let sql_file = match File::create(&sql_path) {
            Ok(f) => f,
            Err(e) => {
                if !silent {
                    error!("Failed to create SQL file for {}: {}", db_name, e);
                }
                db_errors.push((db_name.clone(), format!("Failed to create file: {}", e)));
                continue;
            }
        };
        
        let writer = BufWriter::new(sql_file);
        if let Err(e) = driver.dump_database_silent(db_name, Box::new(writer), silent).await {
            if !silent {
                error!("Failed to dump database {}: {}", db_name, e);
            }
            let _ = fs::remove_file(&sql_path);
            db_errors.push((db_name.clone(), format!("Failed to dump: {}", e)));
            continue;
        }
        
        if !silent {
            info!("Successfully dumped: {}", db_name);
        }
        sql_files.push((sql_path, sql_filename));
        successful_dbs.push(db_name.clone());
    }
    if sql_files.is_empty() {
        return BackupResult {
            connection_name: db_config.name.clone(),
            databases: databases.to_vec(),
            success: false,
            file_path: None,
            file_size: None,
            duration_secs: start.elapsed().as_secs(),
            error: Some("No databases were successfully dumped".to_string()),
            db_errors,
        };
    }
    let zip_filename = format!("backup_{}_{}.zip", db_config.name, timestamp_str);
    let zip_path = backup_dir.join(&zip_filename);
    
    if !silent {
        info!("Creating combined archive with {} databases", sql_files.len());
    }
    
    if let Err(e) = compress_multiple_to_zip_silent(&sql_files, &zip_path, silent) {
        for (sql_path, _) in &sql_files {
            let _ = fs::remove_file(sql_path);
        }
        return BackupResult {
            connection_name: db_config.name.clone(),
            databases: successful_dbs,
            success: false,
            file_path: None,
            file_size: None,
            duration_secs: start.elapsed().as_secs(),
            error: Some(format!("Failed to create archive: {}", e)),
            db_errors,
        };
    }
    for (sql_path, _) in &sql_files {
        let _ = fs::remove_file(sql_path);
    }
    let file_size = fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
    let file_hash = calculate_sha256(&zip_path).ok();

    let duration_secs = start.elapsed().as_secs();
    let metadata = BackupMetadata {
        databases: successful_dbs.clone(),
        connection_name: db_config.name.clone(),
        timestamp,
        file_size,
        file_hash,
        duration_secs,
        file_path: zip_path.to_string_lossy().to_string(),
    };
    let uploaders = create_uploaders(&config.upload);
    for uploader in &uploaders {
        if !silent {
            info!("Uploading combined backup to {}", uploader.name());
        }
        if let Err(e) = uploader.upload_silent(&metadata, &zip_path, silent).await {
            if !silent {
                error!("Failed to upload to {}: {}", uploader.name(), e);
            }
        }
    }

    if !silent {
        info!(
            "Combined backup completed: {} databases, {} seconds, {:.2} MB",
            successful_dbs.len(),
            duration_secs,
            file_size as f64 / 1024.0 / 1024.0
        );
    }

    BackupResult {
        connection_name: db_config.name.clone(),
        databases: successful_dbs,
        success: true,
        file_path: Some(zip_path),
        file_size: Some(file_size),
        duration_secs,
        error: None,
        db_errors,
    }
}

pub async fn execute_all_jobs(config: &AppConfig) -> Vec<BackupResult> {
    let mut results = Vec::new();

    for job in &config.backup_jobs {
        let db_config = match config.databases.iter().find(|d| d.name == job.db_config_name) {
            Some(c) => c,
            None => {
                warn!("Database config '{}' not found for job", job.db_config_name);
                continue;
            }
        };
        let result = execute_job_backup(config, db_config, &job.databases).await;
        results.push(result);
    }

    results
}
