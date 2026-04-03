use metrics::histogram;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::info;

use crate::agents::agent_call::run_dev_agent;
use crate::planner::{PlannerAction, planner};
use crate::server::state::AppState;

pub async fn confidence_worker(state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(1));

    loop {
        ticker.tick().await;

        let report = {
            let mut metrics = state.metrics.lock().await;
            metrics.rotate().await;
            metrics.compute_confidence().await
        };

        // Check planner action
        match planner(&report.score) {
            PlannerAction::TicketCreation => {
                histogram!("error_confidence_score").record(report.score);
                let running = {
                    let mut metrics = state.metrics.lock().await;
                    metrics.is_agent_running().await
                };

                info!("Agent running key exists? {}", running);

                if !running {
                    let summarized_errors = {
                        let mut metrics = state.metrics.lock().await;
                        metrics.set_agent_running(600).await;
                        metrics.fetch_summarized_errors(15).await
                    };

                    let redis_conn = state.metrics.lock().await.conn.clone();
                    let db_conn = state.db.clone();
                    let github_client = state.github.clone();
                    let metrics = state.metrics.clone();

                    info!(
                        "Triggering agent analysis with {} error patterns...",
                        summarized_errors.len()
                    );

                    tokio::spawn(async move {
                        run_dev_agent(
                            summarized_errors,
                            report,
                            redis_conn,
                            metrics, // Arc<Mutex<RedisMetrics>> — correct type
                            db_conn,
                            github_client,
                        )
                        .await;
                    });
                } else {
                    info!("Agent already running, skipping...");
                }
            }
            PlannerAction::Wait => {
                info!("No issues found. Waiting for more information");
            }
        }
    }
}
