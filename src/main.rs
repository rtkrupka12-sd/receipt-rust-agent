mod error;
mod config;
mod processor;

use config::AzureConfig;
use processor::azure::{AzureClient, QueueManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AzureConfig::from_env()?;
    let client = AzureClient::new(config);
    let queue_name = "receipt-requests"; // TODO: Make this configurable via env var

    println!("Rust Receipt Processor started...");

    loop {
        match client.fetch_message(queue_name).await {
            Ok(Some(msg)) => {
                println!("Received message: {}", msg.id);
                
                // TODO: Add actual receipt processing logic here
                println!("Analyzing receipt: {}", msg.body);

                // Delete message if processing is successful or after some number of retries
                match client.delete_message(queue_name, &msg.id, &msg.pop_receipt).await {
                    Ok(_) => println!("Message deleted successfully."),
                    Err(e) => eprintln!("Message deletion failed for {}: {}", msg.id, e),
                }
            }
            Ok(None) => {
                // The queue is empty so wait a while before polling again
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
            Err(e) => {
                // Log and wait before retrying after an error
                eprintln!("Connection Error: {}. Retrying in 30s...", e);
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        }
    }
}
