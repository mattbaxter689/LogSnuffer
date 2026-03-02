use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

use crate::agents::agent_call::run_dev_agent;
use crate::planner::{PlannerAction, planner};
use crate::server::state::AppState;

pub async fn confidence_worker(state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(1));

    loop {
        ticker.tick().await;

        let mut metrics = state.metrics.lock().await;

        // Rotate buckets
        metrics.rotate().await;

        // Compute confidence
        let confidence = metrics.compute_confidence().await;
        println!("Confidence: {:.3}", confidence);

        // Check planner action
        match planner(&confidence) {
            PlannerAction::TicketCreation(val) => {
                if !metrics.is_agent_running().await {
                    metrics.set_agent_running(600).await;

                    // Fetch summarized errors
                    let summarized_errors = metrics.fetch_summarized_errors(15).await;
                    let redis_conn = metrics.conn.clone();
                    let db_conn = state.db.clone();
                    let github_client = state.github.clone();

                    println!(
                        "Triggering agent analysis with {} error patterns...",
                        summarized_errors.len()
                    );

                    tokio::spawn(async move {
                        run_dev_agent(summarized_errors, val, redis_conn, db_conn, github_client)
                            .await;
                    });
                } else {
                    println!("Agent already running, skipping...");
                }
            }
            PlannerAction::Wait => {
                println!("No issues found. Waiting for more information");
            }
        }

        // Release the lock
        drop(metrics);
    }
}
