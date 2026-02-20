use std::time::Duration;
use tokio::time::interval;

// Replace 'your_crate_name' with your actual crate name from Cargo.toml
use logsnuffer::log_generator::log_methods::{LogGenerator, SystemState};
use logsnuffer::log_generator::client::LogClient;

#[tokio::main]
async fn main() {
    println!("🔧 Starting Log Generator Client...");

    let pods = vec![
        "pod-1".into(),
        "pod-2".into(),
        "pod-3".into(),
        "pod-4".into(),
        "pod-5".into(),
    ];
    let apis = vec!["checkout".into(), "cart".into()];
    
    let mut generator = LogGenerator::new(pods, apis);
    let client = LogClient::new("http://localhost:3000");
    
    let mut ticker = interval(Duration::from_secs(1));

    println!("✅ Connected to API at http://localhost:3000");
    println!("🔄 Starting log generation loop...");

    loop {
        ticker.tick().await;

        let log_count = match generator.state {
            SystemState::Incident => 100,
            SystemState::Degraded => 40,
            SystemState::Healthy => 15,
        };

        // Generate the vector of logs (exactly like your original code)
        let logs = generator.log_vec(log_count);
        
        // Send the entire batch via API
        match client.send_logs(logs).await {
            Ok(_) => {
                // Success - optionally log
                // println!("✅ Sent {} logs", log_count);
            }
            Err(e) => {
                eprintln!("❌ Failed to send logs: {}", e);
            }
        }
    }
}
