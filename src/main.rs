mod agents;
mod background;
mod database;
mod log_generator;
mod planner;
mod redis_metrics;
mod server;
mod ticket_tool;
mod utils;

use std::time::Duration;
use tokio::time::interval;

use crate::agents::agent_call::run_dev_agent;
use crate::database::init_db::{init_db, store_log};
use crate::log_generator::log_methods::{LogGenerator, SystemState};
use crate::planner::PlannerAction;
use crate::planner::planner;
use crate::redis_metrics::metrics::RedisMetrics;

#[tokio::main]
async fn main() {
    let db = init_db().await;

    let pods = vec![
        "pod-1".into(),
        "pod-2".into(),
        "pod-3".into(),
        "pod-4".into(),
        "pod-5".into(),
    ];
    let num_pods = pods.len();
    let apis = vec!["checkout".into(), "cart".into()];

    let mut generator = LogGenerator::new(pods, apis);

    // ✅ Add .await here
    let mut metrics = RedisMetrics::new("redis://127.0.0.1/", 30, 0.7, num_pods).await;

    let mut ticker = interval(Duration::from_secs(1));

    loop {
        ticker.tick().await;

        let log_count = match generator.state {
            SystemState::Incident => 100,
            SystemState::Degraded => 40,
            SystemState::Healthy => 15,
        };

        let logs = generator.log_vec(log_count);

        for log in logs.iter() {
            // ✅ Add .await here
            metrics.ingest(log).await;

            // store in database asynchronously
            let db_clone = db.clone();
            let log_clone = log.clone();
            tokio::spawn(async move {
                store_log(db_clone, log_clone).await;
            });
        }

        // ✅ Add .await here
        metrics.rotate().await;

        // Compute confidence purely from Redis
        // ✅ Add .await here
        let confidence = metrics.compute_confidence().await;
        println!("Confidence: {:.3}", confidence);

        match planner(&confidence) {
            PlannerAction::TicketCreation(val) => {
                // Check if agent is currently running
                if !metrics.is_agent_running().await {
                    // ✅ Add .await here
                    metrics.set_agent_running(600).await; // 10 min timeout (safety)

                    // ✅ Add .await here
                    let recent_logs = metrics.fetch_logs_from_window(50).await;
                    let redis_url = "redis://127.0.0.1/"; // Convert to String

                    println!("🎫 Triggering agent analysis...");

                    tokio::spawn(async move {
                        run_dev_agent(recent_logs, val, redis_url).await;
                    });
                } else {
                    println!("⏳ Agent already running, skipping...");
                }
            }
            PlannerAction::Test => {
                println!("Metrics confidence not high enough")
            }
            PlannerAction::Wait => {
                println!("Waiting")
            }
        }
    }
}
