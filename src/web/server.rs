use super::state::AppState;
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::sync::Arc;
use tracing::{error, info};

const DASHBOARD_HTML: &str = include_str!("dashboard.html");

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: T,
}

pub async fn start_server(state: Arc<AppState>, port: u16) {
    let app = Router::new()
        .route("/", get(dashboard_handler))
        .route("/api/status", get(status_handler))
        .route("/api/history", get(history_handler))
        .route("/api/scheduler", get(scheduler_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting web dashboard on http://localhost:{}", port);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind to {}: {}", addr, e);
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        error!("Web server error: {}", e);
    }
}

async fn check_auth(headers: &HeaderMap, state: &AppState) -> bool {
    let auth_header = match headers.get(header::AUTHORIZATION) {
        Some(h) => h,
        None => return false,
    };

    let auth_str = match auth_header.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    if !auth_str.starts_with("Basic ") {
        return false;
    }

    let encoded = &auth_str[6..];
    let decoded = match STANDARD.decode(encoded) {
        Ok(d) => d,
        Err(_) => return false,
    };

    let credentials = match String::from_utf8(decoded) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let parts: Vec<&str> = credentials.splitn(2, ':').collect();
    if parts.len() != 2 {
        return false;
    }

    state.check_credentials(parts[0], parts[1]).await
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"TLM Backup Dashboard\"")],
        "Unauthorized",
    )
        .into_response()
}

async fn dashboard_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !check_auth(&headers, &state).await {
        return unauthorized();
    }
    Html(DASHBOARD_HTML).into_response()
}

async fn status_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !check_auth(&headers, &state).await {
        return unauthorized();
    }

    let scheduler = state.scheduler.read().await;
    let history = state.history.read().await;
    let config = state.config_summary.read().await;

    let total_backups = history.len();
    let successful_backups = history.iter().filter(|b| b.success).count();
    let total_size: u64 = history.iter().map(|b| b.file_size).sum();

    #[derive(Serialize)]
    struct StatusData {
        scheduler_running: bool,
        next_run: Option<String>,
        total_backups: usize,
        successful_backups: usize,
        success_rate: f64,
        total_size_mb: f64,
        database_connections: usize,
        backup_jobs: usize,
        discord_configured: bool,
    }

    let data = StatusData {
        scheduler_running: scheduler.running,
        next_run: scheduler.next_run.map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
        total_backups,
        successful_backups,
        success_rate: if total_backups > 0 {
            (successful_backups as f64 / total_backups as f64) * 100.0
        } else {
            100.0
        },
        total_size_mb: total_size as f64 / 1024.0 / 1024.0,
        database_connections: config.database_connections,
        backup_jobs: config.backup_jobs,
        discord_configured: config.discord_configured,
    };

    Json(ApiResponse { success: true, data }).into_response()
}

async fn history_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !check_auth(&headers, &state).await {
        return unauthorized();
    }

    let history = state.history.read().await;
    Json(ApiResponse {
        success: true,
        data: history.clone(),
    })
    .into_response()
}

async fn scheduler_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !check_auth(&headers, &state).await {
        return unauthorized();
    }

    let scheduler = state.scheduler.read().await;
    Json(ApiResponse {
        success: true,
        data: scheduler.clone(),
    })
    .into_response()
}
