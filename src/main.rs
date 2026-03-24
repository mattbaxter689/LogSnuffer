mod agents;
mod background;
mod database;
mod github;
mod log_generator;
mod planner;
mod redis_metrics;
mod server;
mod state;
mod ticket_tool;

use axum::{
    Router,
    routing::{get, post},
};
use rustls::crypto::{CryptoProvider, ring::default_provider};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;

use crate::database::init_db::init_db;
use crate::github::client::GitHubClient;
use crate::redis_metrics::metrics::RedisMetrics;
use crate::server::handlers::{get_confidence, health_check, ingest_logs};
use crate::server::state::AppState;
use crate::server::webhook::github_webhook;

#[tokio::main]
async fn main() {
    CryptoProvider::install_default(default_provider()).unwrap();

    tracing_subscriber::fmt::init();

    println!("Starting Log Analytics API...");

    let db = init_db().await;
    println!("Database and tables initialized");

    let metrics = RedisMetrics::new(
        &std::env::var("REDIS_URL")
            .expect("REDIS_URL environment variable is missing. Must be set"),
        30,
        0.7,
    )
    .await;
    println!("Connected to Redis");

    let github_token =
        std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN environment variable must be set");
    let github_owner =
        std::env::var("GITHUB_OWNER").expect("GITHUB_OWNER environment variable must be set");
    let github_repo =
        std::env::var("GITHUB_REPO").expect("GITHUB_REPO environment variable must be set");

    let github = GitHubClient::new(&github_token, github_owner, github_repo)
        .expect("Failed to create GitHub client");
    println!("GitHub client initialized");

    let state = Arc::new(AppState {
        db,
        metrics: Arc::new(Mutex::new(metrics)),
        github,
    });

    let worker_state = state.clone();
    tokio::spawn(async move {
        background::worker::confidence_worker(worker_state).await;
    });
    println!("Started background worker");

    let app: Router = Router::new()
        .route("/", get(health_check))
        .route("/health", get(health_check))
        .route("/api/logs", post(ingest_logs))
        .route("/api/confidence", get(get_confidence))
        .route("/webhooks/github", post(github_webhook))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
