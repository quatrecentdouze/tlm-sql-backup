use crate::backup::run_scheduler;
use crate::config::{self, AppConfig};
use crate::database::create_driver;
use crate::error::{BackupError, Result};
use crate::upload::{BackupUploader, DiscordUploader};
use crate::web::{AppState, BackupEntry, ConfigSummary, SchedulerStatus};
use console::style;
use dialoguer::Select;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq)]
enum MenuOption {
    RunBackupNow,
    SchedulerMenu,
    WebDashboardMenu,
    EditConfiguration,
    TestDatabaseConnection,
    TestDiscordUpload,
    Quit,
}

impl MenuOption {
    fn display(&self, scheduler_running: bool, web_running: bool) -> String {
        match self {
            MenuOption::RunBackupNow => "Run backup now (all jobs)".to_string(),
            MenuOption::SchedulerMenu => {
                if scheduler_running {
                    format!("Scheduler [{}]", style("RUNNING").green())
                } else {
                    format!("Scheduler [{}]", style("STOPPED").dim())
                }
            }
            MenuOption::WebDashboardMenu => {
                if web_running {
                    format!("Web Dashboard [{}]", style("RUNNING").green())
                } else {
                    format!("Web Dashboard [{}]", style("STOPPED").dim())
                }
            }
            MenuOption::EditConfiguration => "Edit configuration".to_string(),
            MenuOption::TestDatabaseConnection => "Test database connection".to_string(),
            MenuOption::TestDiscordUpload => "Test Discord upload".to_string(),
            MenuOption::Quit => "Quit".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SchedulerOption {
    Start,
    Stop,
    ViewLogs,
    Back,
}

impl std::fmt::Display for SchedulerOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedulerOption::Start => write!(f, "Start scheduler"),
            SchedulerOption::Stop => write!(f, "Stop scheduler"),
            SchedulerOption::ViewLogs => write!(f, "View scheduler logs"),
            SchedulerOption::Back => write!(f, "Back to main menu"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WebOption {
    Start,
    Stop,
    ViewLogs,
    Back,
}

impl std::fmt::Display for WebOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebOption::Start => write!(f, "Start web dashboard"),
            WebOption::Stop => write!(f, "Stop web dashboard"),
            WebOption::ViewLogs => write!(f, "View dashboard info"),
            WebOption::Back => write!(f, "Back to main menu"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum EditOption {
    DatabaseConnection,
    SelectDatabases,
    ChangeSchedule,
    UploadSettings,
    WebDashboard,
    BackupDirectory,
    Back,
}

impl std::fmt::Display for EditOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditOption::DatabaseConnection => write!(f, "Add/Edit database connection"),
            EditOption::SelectDatabases => write!(f, "Select databases to backup"),
            EditOption::ChangeSchedule => write!(f, "Change backup schedule"),
            EditOption::UploadSettings => write!(f, "Configure Discord upload"),
            EditOption::WebDashboard => write!(f, "Configure web dashboard"),
            EditOption::BackupDirectory => write!(f, "Change backup directory"),
            EditOption::Back => write!(f, "Back to main menu"),
        }
    }
}

fn display_header() {
    println!();
    println!("{}", style("╔════════════════════════════════════════╗").cyan());
    println!("{}", style("║     TLM Database Backup Manager        ║").cyan());
    println!("{}", style("╚════════════════════════════════════════╝").cyan());
    println!();
}

fn display_summary(config: &AppConfig, scheduler_running: bool, web_running: bool) {
    let db_count = config.databases.len();
    let job_count = config.backup_jobs.len();

    println!("{}", style("Current Configuration:").bold());
    println!(
        "  Database connections: {}",
        if db_count > 0 {
            style(db_count.to_string()).green()
        } else {
            style("None".to_string()).red()
        }
    );
    println!(
        "  Backup jobs: {}",
        if job_count > 0 {
            style(job_count.to_string()).green()
        } else {
            style("None".to_string()).red()
        }
    );
    println!(
        "  Discord: {}",
        if config.upload.discord.is_some() {
            style("Configured").green()
        } else {
            style("Not configured").yellow()
        }
    );
    println!(
        "  Scheduler: {}",
        if scheduler_running {
            style("Running").green()
        } else {
            style("Stopped").dim()
        }
    );
    println!(
        "  Web Dashboard: {}",
        if web_running {
            style(format!("Running on http://localhost:{}", config.web.port)).green()
        } else {
            style("Stopped".to_string()).dim()
        }
    );
    println!(
        "  Backup directory: {}",
        style(config.local_backup_dir.display()).cyan()
    );
    println!();
}

struct BackgroundServices {
    scheduler_shutdown: Arc<AtomicBool>,
    scheduler_handle: Option<JoinHandle<()>>,
    web_handle: Option<JoinHandle<()>>,
    web_running: Arc<AtomicBool>,
}

impl BackgroundServices {
    fn new() -> Self {
        Self {
            scheduler_shutdown: Arc::new(AtomicBool::new(false)),
            scheduler_handle: None,
            web_handle: None,
            web_running: Arc::new(AtomicBool::new(false)),
        }
    }

    fn is_scheduler_running(&self) -> bool {
        self.scheduler_handle.as_ref().map_or(false, |h| !h.is_finished())
    }

    fn is_web_running(&self) -> bool {
        self.web_running.load(Ordering::Relaxed)
    }
}

pub async fn run_menu(shutdown: Arc<AtomicUsize>, app_state: Arc<AppState>) -> Result<()> {
    let mut config = config::load()?;
    let mut services = BackgroundServices::new();
    if config.databases.is_empty() {
        println!(
            "\n{}",
            style("No configuration found. Starting setup wizard...").yellow()
        );
        super::wizard::run_initial_setup(&mut config).await?;
        config::save(&config)?;
    }
    update_config_summary(&config, &app_state).await;

    loop {
        if shutdown.load(Ordering::Relaxed) > 0 {
            if services.is_scheduler_running() {
                services.scheduler_shutdown.store(true, Ordering::SeqCst);
            }
            break;
        }

        display_header();
        display_summary(&config, services.is_scheduler_running(), services.is_web_running());

        let menu_items = vec![
            MenuOption::RunBackupNow,
            MenuOption::SchedulerMenu,
            MenuOption::WebDashboardMenu,
            MenuOption::EditConfiguration,
            MenuOption::TestDatabaseConnection,
            MenuOption::TestDiscordUpload,
            MenuOption::Quit,
        ];

        let display_items: Vec<String> = menu_items
            .iter()
            .map(|m| m.display(services.is_scheduler_running(), services.is_web_running()))
            .collect();

        let selection = match Select::new()
            .with_prompt("Select an option")
            .items(&display_items)
            .default(0)
            .interact_opt()
        {
            Ok(Some(s)) => s,
            Ok(None) => break,
            Err(_) => break,
        };

        match menu_items[selection] {
            MenuOption::RunBackupNow => {
                run_backup_now(&config, app_state.clone()).await;
            }
            MenuOption::SchedulerMenu => {
                scheduler_menu(&config, &mut services, app_state.clone()).await;
            }
            MenuOption::WebDashboardMenu => {
                web_dashboard_menu(&mut config, &mut services, app_state.clone()).await;
            }
            MenuOption::EditConfiguration => {
                if let Err(e) = edit_configuration(&mut config).await {
                    println!("{}: {}", style("Error").red(), e);
                } else {
                    let _ = config::save(&config);
                    update_config_summary(&config, &app_state).await;
                }
            }
            MenuOption::TestDatabaseConnection => {
                test_database_connection(&config).await;
            }
            MenuOption::TestDiscordUpload => {
                test_discord_upload(&config).await;
            }
            MenuOption::Quit => {
                if services.is_scheduler_running() {
                    println!("{}", style("Stopping scheduler...").yellow());
                    services.scheduler_shutdown.store(true, Ordering::SeqCst);
                }
                println!("{}", style("Goodbye!").green());
                break;
            }
        }
    }

    Ok(())
}

async fn update_config_summary(config: &AppConfig, app_state: &Arc<AppState>) {
    app_state.update_config(ConfigSummary {
        database_connections: config.databases.len(),
        backup_jobs: config.backup_jobs.len(),
        discord_configured: config.upload.discord.is_some(),
        backup_directory: config.local_backup_dir.to_string_lossy().to_string(),
    }).await;
}

async fn scheduler_menu(config: &AppConfig, services: &mut BackgroundServices, app_state: Arc<AppState>) {
    loop {
        println!("\n{}", style("=== Scheduler ===").cyan().bold());
        
        let is_running = services.is_scheduler_running();
        println!(
            "Status: {}",
            if is_running {
                style("Running").green()
            } else {
                style("Stopped").dim()
            }
        );

        let options = vec![
            SchedulerOption::Start,
            SchedulerOption::Stop,
            SchedulerOption::ViewLogs,
            SchedulerOption::Back,
        ];

        let selection = match Select::new()
            .with_prompt("Select action")
            .items(&options)
            .default(0)
            .interact_opt()
        {
            Ok(Some(s)) => s,
            Ok(None) | Err(_) => break,
        };

        match options[selection] {
            SchedulerOption::Start => {
                if is_running {
                    println!("{}", style("Scheduler is already running!").yellow());
                } else if config.backup_jobs.is_empty() {
                    println!("{}", style("No backup jobs configured. Please configure databases first.").red());
                } else {
                    services.scheduler_shutdown.store(false, Ordering::SeqCst);
                    
                    let config_arc = Arc::new(config.clone());
                    let shutdown = services.scheduler_shutdown.clone();
                    let state = app_state.clone();
                    let shutdown_usize = Arc::new(AtomicUsize::new(0));
                    let shutdown_usize_clone = shutdown_usize.clone();
                    let shutdown_clone = shutdown.clone();
                    tokio::spawn(async move {
                        loop {
                            if shutdown_clone.load(Ordering::Relaxed) {
                                shutdown_usize_clone.store(1, Ordering::SeqCst);
                                break;
                            }
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    });
                    
                    services.scheduler_handle = Some(tokio::spawn(async move {
                        run_scheduler(config_arc, shutdown_usize, state).await;
                    }));
                    
                    println!("{}", style("Scheduler started!").green());
                }
            }
            SchedulerOption::Stop => {
                if !is_running {
                    println!("{}", style("Scheduler is not running.").yellow());
                } else {
                    services.scheduler_shutdown.store(true, Ordering::SeqCst);
                    app_state.update_scheduler(SchedulerStatus {
                        running: false,
                        next_run: None,
                        interval_secs: 0,
                        connection_name: None,
                        database_count: 0,
                    }).await;
                    
                    println!("{}", style("Scheduler stop signal sent!").green());
                }
            }
            SchedulerOption::ViewLogs => {
                println!("\n{}", style("=== Live Scheduler Logs (press 'q' to exit) ===").cyan().bold());
                
                loop {
                    print!("\x1B[2J\x1B[1;1H");
                    println!("{}", style("=== Live Scheduler Logs (press 'q' to exit) ===").cyan().bold());
                    let scheduler = app_state.scheduler.read().await;
                    println!("\n{}", style("Status:").cyan());
                    println!("  Running: {}", if scheduler.running { style("Yes").green() } else { style("No").dim() });
                    if let Some(ref next) = scheduler.next_run {
                        println!("  Next run: {}", style(next.format("%Y-%m-%d %H:%M:%S UTC")).cyan());
                    }
                    println!("  Interval: {} seconds", scheduler.interval_secs);
                    if let Some(ref conn) = scheduler.connection_name {
                        println!("  Connection: {}", style(conn).cyan());
                    }
                    println!("  Databases: {}", scheduler.database_count);
                    drop(scheduler);

                    println!("\n{}", style("Recent Logs:").cyan());
                    let logs = app_state.scheduler_logs.read().await;
                    if logs.is_empty() {
                        println!("  {}", style("No logs yet").dim());
                    } else {
                        for log in logs.iter().take(15) {
                            let level_style = match log.level.as_str() {
                                "ERROR" => style(&log.level).red(),
                                "WARN" => style(&log.level).yellow(),
                                _ => style(&log.level).cyan(),
                            };
                            println!(
                                "  {} [{}] {}",
                                style(log.timestamp.format("%H:%M:%S")).dim(),
                                level_style,
                                log.message
                            );
                        }
                    }
                    drop(logs);
                    
                    println!("\n{}", style("Press 'q' to return to menu...").dim());
                    let should_exit = tokio::select! {
                        result = tokio::task::spawn_blocking(|| {
                            use std::io::Read;
                            if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(100)) {
                                if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                                    if key.code == crossterm::event::KeyCode::Char('q') {
                                        return true;
                                    }
                                }
                            }
                            false
                        }) => result.unwrap_or(false),
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => false,
                    };
                    
                    if should_exit {
                        break;
                    }
                }
            }
            SchedulerOption::Back => break,
        }
    }
}

async fn web_dashboard_menu(config: &mut AppConfig, services: &mut BackgroundServices, app_state: Arc<AppState>) {
    loop {
        println!("\n{}", style("=== Web Dashboard ===").cyan().bold());
        
        let is_running = services.is_web_running();
        println!(
            "Status: {}",
            if is_running {
                style(format!("Running on http://localhost:{}", config.web.port)).green()
            } else {
                style("Stopped".to_string()).dim()
            }
        );

        let options = vec![
            WebOption::Start,
            WebOption::Stop,
            WebOption::ViewLogs,
            WebOption::Back,
        ];

        let selection = match Select::new()
            .with_prompt("Select action")
            .items(&options)
            .default(0)
            .interact_opt()
        {
            Ok(Some(s)) => s,
            Ok(None) | Err(_) => break,
        };

        match options[selection] {
            WebOption::Start => {
                if is_running {
                    println!("{}", style("Web dashboard is already running!").yellow());
                } else if !config.web.enabled {
                    println!("{}", style("Web dashboard is not configured. Please configure it first.").red());
                } else if config.web.username.is_empty() || config.web.password.is_empty() {
                    println!("{}", style("Web dashboard credentials not set. Please configure them first.").red());
                } else {
                    app_state.set_credentials(config.web.username.clone(), config.web.password.clone()).await;
                    app_state.update_config(ConfigSummary {
                        database_connections: config.databases.len(),
                        backup_jobs: config.backup_jobs.len(),
                        discord_configured: config.upload.discord.is_some(),
                        backup_directory: config.local_backup_dir.to_string_lossy().to_string(),
                    }).await;

                    let port = config.web.port;
                    let state = app_state.clone();
                    let running = services.web_running.clone();
                    running.store(true, Ordering::SeqCst);
                    
                    services.web_handle = Some(tokio::spawn(async move {
                        crate::web::start_server(state, port).await;
                        running.store(false, Ordering::SeqCst);
                    }));
                    
                    println!(
                        "{}",
                        style(format!("Web dashboard started on http://localhost:{}", port)).green()
                    );
                    println!(
                        "  Username: {}, Password: {}",
                        style(&config.web.username).cyan(),
                        style("*****").dim()
                    );
                }
            }
            WebOption::Stop => {
                if !is_running {
                    println!("{}", style("Web dashboard is not running.").yellow());
                } else {

                    services.web_running.store(false, Ordering::SeqCst);
                    if let Some(handle) = services.web_handle.take() {
                        handle.abort();
                    }
                    println!("{}", style("Web dashboard stopped!").green());
                }
            }
            WebOption::ViewLogs => {
                println!("\n{}", style("=== Web Dashboard Info ===").cyan());
                println!("  Status: {}", if is_running { style("Running").green() } else { style("Stopped").dim() });
                if is_running {
                    println!("  URL: {}", style(format!("http://localhost:{}", config.web.port)).cyan());
                    println!("  Username: {}", style(&config.web.username).cyan());
                }
                
                let history = app_state.history.read().await;
                println!("\n  Recent backups in dashboard: {}", history.len());
                
                println!("\n{}", style("Press Enter to return to menu...").dim());
                let _ = tokio::task::spawn_blocking(|| {
                    let _ = std::io::stdin().read_line(&mut String::new());
                }).await;
            }
            WebOption::Back => break,
        }
    }
}

async fn run_backup_now(config: &AppConfig, app_state: Arc<AppState>) {
    println!("\n{}", style("Running all backup jobs...").yellow());

    if config.backup_jobs.is_empty() {
        println!(
            "{}",
            style("No backup jobs configured. Please configure databases first.").red()
        );
        return;
    }

    let results = crate::backup::execute_all_jobs(config).await;

    println!("\n{}", style("=== Backup Results ===").cyan().bold());
    for result in &results {
        app_state.add_backup_entry(BackupEntry {
            timestamp: chrono::Utc::now(),
            connection_name: result.connection_name.clone(),
            databases: result.databases.clone(),
            success: result.success,
            file_size: result.file_size.unwrap_or(0),
            duration_secs: result.duration_secs,
            error: result.error.clone(),
        }).await;
        
        if result.success {
            println!(
                "{} {} ({} databases) - {} ({:.2} MB, {} sec)",
                style("✓").green(),
                result.connection_name,
                result.databases.len(),
                style("Success").green(),
                result.file_size.unwrap_or(0) as f64 / 1024.0 / 1024.0,
                result.duration_secs
            );
            println!("    Databases: {}", result.databases.join(", "));
        } else {
            println!(
                "{} {} - {} ({})",
                style("✗").red(),
                result.connection_name,
                style("Failed").red(),
                result.error.as_deref().unwrap_or("Unknown error")
            );
        }
        for (db_name, err) in &result.db_errors {
            println!("    {} {}: {}", style("⚠").yellow(), db_name, err);
        }
    }

    let success_count = results.iter().filter(|r| r.success).count();
    println!(
        "\nCompleted: {}/{} backup jobs successful",
        style(success_count).green(),
        results.len()
    );

    println!("\nPress Enter to continue...");
    let _ = std::io::stdin().read_line(&mut String::new());
}

async fn edit_configuration(config: &mut AppConfig) -> Result<()> {
    loop {
        println!("\n{}", style("=== Edit Configuration ===").cyan().bold());

        let edit_items = vec![
            EditOption::DatabaseConnection,
            EditOption::SelectDatabases,
            EditOption::ChangeSchedule,
            EditOption::UploadSettings,
            EditOption::WebDashboard,
            EditOption::BackupDirectory,
            EditOption::Back,
        ];

        let selection = match Select::new()
            .with_prompt("What would you like to edit?")
            .items(&edit_items)
            .default(0)
            .interact_opt()
        {
            Ok(Some(s)) => s,
            Ok(None) | Err(_) => break,
        };

        match edit_items[selection] {
            EditOption::DatabaseConnection => {
                super::wizard::configure_database(config).await?;
            }
            EditOption::SelectDatabases => {
                super::wizard::select_databases(config).await?;
            }
            EditOption::ChangeSchedule => {
                if config.backup_jobs.is_empty() {
                    println!(
                        "{}",
                        style("No backup jobs configured yet. Please select databases first.").red()
                    );
                } else {
                    let schedule = super::wizard::configure_schedule()?;
                    for job in &mut config.backup_jobs {
                        job.schedule = schedule.clone();
                    }
                    println!("{}", style("Schedule updated for all jobs.").green());
                }
            }
            EditOption::UploadSettings => {
                super::wizard::configure_discord(config).await?;
            }
            EditOption::WebDashboard => {
                super::wizard::configure_web_dashboard(config)?;
            }
            EditOption::BackupDirectory => {
                super::wizard::configure_backup_directory(config)?;
            }
            EditOption::Back => {
                break;
            }
        }
    }

    Ok(())
}

async fn test_database_connection(config: &AppConfig) {
    if config.databases.is_empty() {
        println!(
            "{}",
            style("No database connections configured.").red()
        );
        return;
    }

    println!("\n{}", style("Testing database connections...").yellow());

    for db_config in &config.databases {
        print!("  {} ({})... ", db_config.name, db_config.engine);
        match create_driver(db_config) {
            Ok(driver) => match driver.test_connection().await {
                Ok(_) => println!("{}", style("OK").green()),
                Err(e) => println!("{}: {}", style("FAILED").red(), e),
            },
            Err(e) => println!("{}: {}", style("ERROR").red(), e),
        }
    }

    println!("\nPress Enter to continue...");
    let _ = std::io::stdin().read_line(&mut String::new());
}

async fn test_discord_upload(config: &AppConfig) {
    match &config.upload.discord {
        Some(discord_config) => {
            println!("\n{}", style("Testing Discord connection...").yellow());
            let uploader = DiscordUploader::new(discord_config);
            match uploader.test_connection().await {
                Ok(_) => println!("{}", style("Discord connection successful!").green()),
                Err(e) => println!("{}: {}", style("Discord test failed").red(), e),
            }
        }
        None => {
            println!(
                "{}",
                style("Discord is not configured. Please configure it first.").red()
            );
        }
    }

    println!("\nPress Enter to continue...");
    let _ = std::io::stdin().read_line(&mut String::new());
}
