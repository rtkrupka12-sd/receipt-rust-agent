mod error;
mod config;
mod processor;

use config::AzureConfig;
use processor::AzureClient;
use processor::azure_queue::{QueueManager, QueueMessage};
use processor::azure_container::BlobManager;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration and initialize client
    let config = AzureConfig::from_env()?;
    let client = AzureClient::new(config);
    let queue_name = "receipt-requests"; // TODO: Make this configurable via env var

    println!("Rust Receipt Processor started...");

    loop {
        match client.fetch_message(queue_name).await {
            Ok(Some(msg)) => {
                println!("Received message: {}", msg.id);

                println!("Analyzing receipt...");
                if let Err(e) = process_workflow(&client, msg).await {
                    eprintln!("Failed to process message: {:?}", e);
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

async fn process_workflow(client: &AzureClient, msg: QueueMessage) -> Result<(), crate::error::ProcessorError> {
    // TODO: Add more logic to determine blob name
    let blob_name = &msg.body;

    // Download the blob
    println!("Downloading blob: {}", blob_name);
    let blob_data = client.download_blob("receipts", blob_name).await?;
    println!("Downloaded {} bytes", blob_data.len());

    // TODO: Add OCR processing logic
    println!("Simulating OCR processing for {}...", blob_name);
    
    // Update Metadata
    let mut metadata = HashMap::new();
    metadata.insert("ProcessingStatus".to_string(), "Completed".to_string());
    metadata.insert("ProcessedAt".to_string(), "2024-05-20T12:00:00Z".to_string()); // Placeholder timestamp
    
    client.update_metadata("receipts", blob_name, metadata).await?;
    println!("Metadata updated for {}", blob_name);

    // Delete from Queue
    client.delete_message("receipt-requests", &msg.id, &msg.pop_receipt).await?;
    println!("Message {} deleted successfully.", msg.id);

    Ok(())
}
