use crate::config::AppConfig;
use crate::web::{AppState, BackupEntry, SchedulerStatus};
use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::select;
use tokio::time::sleep;
pub async fn run_scheduler(config: Arc<AppConfig>, shutdown: Arc<AtomicUsize>, app_state: Arc<AppState>) {
    app_state.add_log("INFO", "Starting backup scheduler").await;

    if config.backup_jobs.is_empty() {
        app_state.add_log("WARN", "No backup jobs configured. Scheduler will wait for configuration.").await;
    }
    let min_interval = config
        .backup_jobs
        .iter()
        .map(|j| j.schedule.as_seconds())
        .min()
        .unwrap_or(3600);

    app_state.add_log("INFO", &format!("Scheduler interval: {} seconds", min_interval)).await;
    let mut last_run: std::collections::HashMap<String, std::time::Instant> = 
        std::collections::HashMap::new();
    let mut first_run = true;
    
    loop {
        if shutdown.load(Ordering::Relaxed) > 0 {
            app_state.update_scheduler(SchedulerStatus {
                running: false,
                next_run: None,
                interval_secs: min_interval,
                connection_name: None,
                database_count: 0,
            }).await;
            app_state.add_log("INFO", "Scheduler shutdown requested").await;
            break;
        }
        if !first_run {
            let next_run = Utc::now() + Duration::seconds(min_interval as i64);
            app_state.update_scheduler(SchedulerStatus {
                running: true,
                next_run: Some(next_run),
                interval_secs: min_interval,
                connection_name: config.backup_jobs.first().map(|j| j.db_config_name.clone()),
                database_count: config.backup_jobs.iter().map(|j| j.databases.len()).sum(),
            }).await;
            select! {
                _ = sleep(std::time::Duration::from_secs(min_interval)) => {}
                _ = async {
                    while shutdown.load(Ordering::Relaxed) == 0 {
                        sleep(std::time::Duration::from_millis(100)).await;
                    }
                } => {
                    app_state.add_log("INFO", "Scheduler shutdown requested during wait").await;
                    break;
                }
            }
            if shutdown.load(Ordering::Relaxed) > 0 {
                app_state.update_scheduler(SchedulerStatus {
                    running: false,
                    next_run: None,
                    interval_secs: min_interval,
                    connection_name: None,
                    database_count: 0,
                }).await;
                app_state.add_log("INFO", "Scheduler shutdown requested").await;
                break;
            }
        } else {
            app_state.update_scheduler(SchedulerStatus {
                running: true,
                next_run: None,
                interval_secs: min_interval,
                connection_name: config.backup_jobs.first().map(|j| j.db_config_name.clone()),
                database_count: config.backup_jobs.iter().map(|j| j.databases.len()).sum(),
            }).await;
        }
        first_run = false;

        if config.backup_jobs.is_empty() {
            continue;
        }

        let now = std::time::Instant::now();
        for job in &config.backup_jobs {
            let job_key = format!("{}:{:?}", job.db_config_name, job.databases);
            let interval_secs = job.schedule.as_seconds();

            let should_run = match last_run.get(&job_key) {
                Some(last) => now.duration_since(*last).as_secs() >= interval_secs,
                None => true,
            };

            if should_run {
                app_state.add_log("INFO", &format!("Executing backup job for {}", job.db_config_name)).await;
                if let Some(db_config) = config.databases.iter().find(|d| d.name == job.db_config_name) {
                    let result = crate::backup::job::execute_job_backup_silent(&config, db_config, &job.databases).await;
                    app_state.add_backup_entry(BackupEntry {
                        timestamp: Utc::now(),
                        connection_name: result.connection_name.clone(),
                        databases: result.databases.clone(),
                        success: result.success,
                        file_size: result.file_size.unwrap_or(0),
                        duration_secs: result.duration_secs,
                        error: result.error.clone(),
                    }).await;
                    
                    if result.success {
                        app_state.add_log("INFO", &format!(
                            "Backup of {} ({} databases) completed: {:.2} MB in {} sec",
                            result.connection_name,
                            result.databases.len(),
                            result.file_size.unwrap_or(0) as f64 / 1024.0 / 1024.0,
                            result.duration_secs
                        )).await;
                    } else {
                        app_state.add_log("ERROR", &format!(
                            "Backup of {} failed: {}",
                            result.connection_name,
                            result.error.unwrap_or_default()
                        )).await;
                    }
                } else {
                    app_state.add_log("WARN", &format!("Database config '{}' not found", job.db_config_name)).await;
                }

                last_run.insert(job_key, now);
            }
        }
    }

    app_state.add_log("INFO", "Scheduler stopped").await;
}
