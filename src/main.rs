mod agents;
mod database;
mod log_generator;
mod planner;
mod redis_metrics;
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
    let mut metrics = RedisMetrics::new("redis://127.0.0.1/", 30, 0.7, num_pods);
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
            metrics.ingest(log);

            // store in database asynchronously
            let db_clone = db.clone();
            let log_clone = log.clone();
            tokio::spawn(async move {
                store_log(db_clone, log_clone).await;
            });
        }
        metrics.rotate();

        // Compute confidence purely from Redis
        let confidence = metrics.compute_confidence();
        println!("Confidence, {:.3}", confidence);
        // match planner(&confidence) {
        //     PlannerAction::TicketCreation(val) => {
        //         if !metrics.check_cooldown("cooldown:user_ticket") {
        //             metrics.set_cooldown("cooldown:user_ticket", 10);
        //             let logs_clone = recent_logs.clone();
        //             tokio::spawn(async move {
        //                 run_dev_agent(logs_clone, val).await;
        //             });
        //         }
        //     }
        //     PlannerAction::Test => {
        //         println!("Metrics not confidence high enough")
        //     }
        //     PlannerAction::Wait => {
        //         println!("Waiting")
        //     }
        // }
    }
}
