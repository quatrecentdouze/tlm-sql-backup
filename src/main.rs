mod backup;
mod cli;
mod config;
mod database;
mod error;
mod log;
mod upload;
mod web;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::info;
use web::AppState;

#[tokio::main]
async fn main() {
    log::init();

    info!("TLM Database Backup CLI starting...");

    let ctrl_c_count = Arc::new(AtomicUsize::new(0));
    let ctrl_c_count_clone = ctrl_c_count.clone();

    ctrlc::set_handler(move || {
        let count = ctrl_c_count_clone.fetch_add(1, Ordering::SeqCst);
        
        if count == 0 {
            println!("\n\nShutdown signal received. Press Ctrl+C again to force exit...");
        } else {
            println!("\nForce exiting...");
            std::process::exit(130);
        }
    })
    .expect("Error setting Ctrl-C handler");

    let app_state = AppState::new(String::new(), String::new());

    match cli::run_menu(ctrl_c_count, app_state).await {
        Ok(_) => {
            info!("Application exited normally");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
