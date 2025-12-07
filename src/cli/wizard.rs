use crate::config::{
    AppConfig, BackupJob, DatabaseConfig, DatabaseEngine, DiscordConfig, Schedule,
};
use crate::database::create_driver;
use crate::error::{BackupError, Result};
use crate::upload::BackupUploader;
use console::style;
use dialoguer::{Input, MultiSelect, Password, Select};
use std::path::PathBuf;

pub async fn configure_database(config: &mut AppConfig) -> Result<()> {
    println!("\n{}", style("=== Database Configuration ===").cyan().bold());

    let name: String = Input::new()
        .with_prompt("Connection name (e.g., 'production', 'local')")
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;
    if config.databases.iter().any(|d| d.name == name) {
        let overwrite = Select::new()
            .with_prompt(&format!("Connection '{}' already exists. Overwrite?", name))
            .items(&["Yes", "No"])
            .default(1)
            .interact()
            .map_err(|e| BackupError::Config(e.to_string()))?;

        if overwrite == 1 {
            return Ok(());
        }
        config.databases.retain(|d| d.name != name);
    }

    let engines = vec!["MySQL"];
    let engine_idx = Select::new()
        .with_prompt("Database engine")
        .items(&engines)
        .default(0)
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let engine = match engine_idx {
        0 => DatabaseEngine::MySQL,
        _ => DatabaseEngine::MySQL,
    };

    let host: String = Input::new()
        .with_prompt("Host")
        .default("localhost".to_string())
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let port: u16 = Input::new()
        .with_prompt("Port")
        .default(3306u16)
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let username: String = Input::new()
        .with_prompt("Username")
        .default("root".to_string())
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let password: String = Password::new()
        .with_prompt("Password")
        .allow_empty_password(true)
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let db_config = DatabaseConfig {
        name: name.clone(),
        engine,
        host,
        port,
        username,
        password,
    };
    println!("\n{}", style("Testing connection...").yellow());
    let driver = create_driver(&db_config)?;
    driver.test_connection().await?;
    println!("{}", style("✓ Connection successful!").green());

    config.databases.push(db_config);
    println!("{}", style(format!("Database connection '{}' added.", name)).green());

    Ok(())
}

pub async fn select_databases(config: &mut AppConfig) -> Result<()> {
    if config.databases.is_empty() {
        println!("{}", style("No database connections configured. Please add one first.").red());
        return Ok(());
    }

    println!("\n{}", style("=== Select Databases to Backup ===").cyan().bold());
    let connection_names: Vec<&str> = config.databases.iter().map(|d| d.name.as_str()).collect();
    let conn_idx = Select::new()
        .with_prompt("Select database connection")
        .items(&connection_names)
        .default(0)
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let db_config = &config.databases[conn_idx];
    let driver = create_driver(db_config)?;
    println!("{}", style("Fetching database list...").yellow());
    let available_dbs = driver.list_databases().await?;

    if available_dbs.is_empty() {
        println!("{}", style("No databases found on this server.").red());
        return Ok(());
    }

    let db_names: Vec<&str> = available_dbs.iter().map(|s| s.as_str()).collect();
    let selected_indices = MultiSelect::new()
        .with_prompt("Select databases to backup (Space to select, Enter to confirm)")
        .items(&db_names)
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    if selected_indices.is_empty() {
        println!("{}", style("No databases selected.").yellow());
        return Ok(());
    }

    let selected_dbs: Vec<String> = selected_indices
        .iter()
        .map(|&i| available_dbs[i].clone())
        .collect();

    println!(
        "{}",
        style(format!("Selected {} database(s)", selected_dbs.len())).green()
    );
    let schedule = configure_schedule()?;
    let job_exists = config
        .backup_jobs
        .iter_mut()
        .find(|j| j.db_config_name == db_config.name);

    if let Some(job) = job_exists {
        job.databases = selected_dbs;
        job.schedule = schedule;
    } else {
        config.backup_jobs.push(BackupJob {
            db_config_name: db_config.name.clone(),
            databases: selected_dbs,
            schedule,
        });
    }

    println!("{}", style("Backup job configured.").green());
    Ok(())
}

pub fn configure_schedule() -> Result<Schedule> {
    println!("\n{}", style("=== Backup Schedule ===").cyan().bold());

    let schedule_types = vec!["Every N minutes", "Every N hours", "Every N days"];
    let type_idx = Select::new()
        .with_prompt("Schedule type")
        .items(&schedule_types)
        .default(1)
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let value: u32 = Input::new()
        .with_prompt("Interval value")
        .default(1u32)
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let schedule = match type_idx {
        0 => Schedule::Minutes(value),
        1 => Schedule::Hours(value),
        2 => Schedule::Days(value),
        _ => Schedule::Hours(1),
    };

    println!("{}", style(format!("Schedule: {}", schedule)).green());
    Ok(schedule)
}

pub async fn configure_discord(config: &mut AppConfig) -> Result<()> {
    println!("\n{}", style("=== Discord Configuration ===").cyan().bold());

    let bot_token: String = Password::new()
        .with_prompt("Discord Bot Token")
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let guild_id: u64 = Input::new()
        .with_prompt("Guild (Server) ID")
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let forum_channel_name: String = Input::new()
        .with_prompt("Forum channel name (will be created if doesn't exist)")
        .default("database-backups".to_string())
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let discord_config = DiscordConfig {
        bot_token,
        guild_id,
        forum_channel_name,
    };
    println!("\n{}", style("Testing Discord connection...").yellow());
    let uploader = crate::upload::DiscordUploader::new(&discord_config);
    uploader.test_connection().await?;
    println!("{}", style("✓ Discord connection successful!").green());

    config.upload.discord = Some(discord_config);
    println!("{}", style("Discord configuration saved.").green());

    Ok(())
}

pub fn configure_backup_directory(config: &mut AppConfig) -> Result<()> {
    println!("\n{}", style("=== Backup Directory ===").cyan().bold());

    let current = config.local_backup_dir.to_string_lossy().to_string();
    let path: String = Input::new()
        .with_prompt("Local backup directory")
        .default(current)
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    config.local_backup_dir = PathBuf::from(path);
    println!(
        "{}",
        style(format!(
            "Backup directory set to: {}",
            config.local_backup_dir.display()
        ))
        .green()
    );

    Ok(())
}

pub fn configure_web_dashboard(config: &mut AppConfig) -> Result<()> {
    println!("\n{}", style("=== Web Dashboard Configuration ===").cyan().bold());

    let enabled = Select::new()
        .with_prompt("Enable web dashboard?")
        .items(&["Yes", "No"])
        .default(if config.web.enabled { 0 } else { 1 })
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    config.web.enabled = enabled == 0;

    if !config.web.enabled {
        println!("{}", style("Web dashboard disabled.").yellow());
        return Ok(());
    }

    let port: u16 = Input::new()
        .with_prompt("Port")
        .default(config.web.port)
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let username: String = Input::new()
        .with_prompt("Username")
        .default(if config.web.username.is_empty() { "admin".to_string() } else { config.web.username.clone() })
        .interact_text()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    let password: String = Password::new()
        .with_prompt("Password")
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    config.web.port = port;
    config.web.username = username;
    config.web.password = password;

    println!(
        "{}",
        style(format!(
            "Web dashboard configured on port {}. Access at http://localhost:{}",
            config.web.port,
            config.web.port
        ))
        .green()
    );

    Ok(())
}

pub async fn run_initial_setup(config: &mut AppConfig) -> Result<()> {
    println!("\n{}", style("╔════════════════════════════════════════╗").cyan());
    println!("{}", style("║     TLM Database Backup - Setup        ║").cyan());
    println!("{}", style("╚════════════════════════════════════════╝").cyan());

    println!("\nWelcome! Let's configure your backup settings.\n");
    configure_database(config).await?;
    select_databases(config).await?;
    configure_backup_directory(config)?;
    let setup_discord = Select::new()
        .with_prompt("Would you like to configure Discord upload?")
        .items(&["Yes", "No"])
        .default(0)
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    if setup_discord == 0 {
        configure_discord(config).await?;
    }
    let setup_web = Select::new()
        .with_prompt("Would you like to configure web dashboard?")
        .items(&["Yes", "No"])
        .default(0)
        .interact()
        .map_err(|e| BackupError::Config(e.to_string()))?;

    if setup_web == 0 {
        configure_web_dashboard(config)?;
    }

    println!("\n{}", style("Setup complete!").green().bold());
    Ok(())
}
