use axum::{
    Extension, Json,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::IntoResponse,
};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::error;

use crate::database::init_db::store_log;
use crate::log_generator::log_methods::LogEntry;
use crate::server::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    message: String,
}

#[derive(Deserialize)]
pub struct IngestRequest {
    pub logs: Vec<LogEntry>,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub ingested: usize,
    pub message: String,
}

#[derive(Serialize)]
pub struct ConfidenceResponse {
    pub confidence: f64,
    pub message: String,
}

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "OK".to_string(),
        message: "Log Analytics API is running".to_string(),
    })
}

pub async fn ingest_logs(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, StatusCode> {
    let count = payload.logs.len();

    if count == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut metrics = state.metrics.lock().await;

    for log in payload.logs.iter() {
        metrics.ingest(log).await;

        let db = state.db.clone();
        let log_clone = log.clone();
        tokio::spawn(async move {
            if let Err(e) = store_log(db, log_clone).await {
                error!("Error storing log: {}", e);
            }
        });
    }

    Ok(Json(IngestResponse {
        ingested: count,
        message: format!("Successfully ingest {} logs", count),
    }))
}

pub async fn get_confidence(State(state): State<Arc<AppState>>) -> Json<ConfidenceResponse> {
    let metrics = state.metrics.lock().await;
    let confidence = metrics.prev_confidence;

    Json(ConfidenceResponse {
        confidence,
        message: format!("Current Confidence: {:.3}", confidence),
    })
}

pub async fn metrics_handler(Extension(handle): Extension<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

pub async fn metrics_middleware(req: Request<axum::body::Body>, next: Next) -> impl IntoResponse {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    let start = Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16().to_string();
    let duration = start.elapsed().as_secs_f64();

    counter!(
        "http_requests_total",
        "method" => method.clone(),
        "status" => status.clone()
    )
    .increment(1);

    histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => path,
        "status" => status
    )
    .record(duration);

    response
}
