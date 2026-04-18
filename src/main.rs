mod error;
mod config;
mod processor;

use config::AzureConfig;
use processor::AzureClient;
use processor::azure_queue::{QueueManager, QueueMessage};
use processor::azure_container::BlobManager;
use processor::ocr::OcrEngine;
use processor::ocr::doc_intel::DocIntelClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = AzureConfig::from_env()?;

    let storage_client = AzureClient::new(config.clone());
    let ocr_client = DocIntelClient::new(
        config.doc_intel_endpoint.clone(), 
        config.doc_intel_key.clone()
    );

    let queue_name = "receipt-requests"; // TODO: Make this configurable via env var

    println!("Rust Receipt Processor started...");

    loop {
        match storage_client.fetch_message(queue_name).await {
            Ok(Some(msg)) => {
                println!("Received message: {}", msg.id);

                println!("Analyzing receipt...");
                if let Err(e) = process_workflow(&storage_client, &ocr_client, msg).await {
                    eprintln!("Workflow error: {:?}", e);
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

async fn process_workflow(storage: &AzureClient, ocr: &DocIntelClient, msg: QueueMessage) -> Result<(), crate::error::ProcessorError> {
    // TODO: Add more logic to determine blob name
    let blob_name = &msg.body;
    let image_bytes = storage.download_blob("receipts", blob_name).await?;

    // Download the blob
    println!("Downloading blob: {}", blob_name);
    let blob_data = storage.download_blob("receipts", blob_name).await?;
    println!("Downloaded {} bytes", blob_data.len());

    // Process with OCR
    println!("Sending {} to Azure Document Intelligence...", blob_name);
    let ocr_result = ocr.process_receipt(image_bytes).await?;
    
    println!("OCR Result: Vendor: {:?}, Amount: {:?}, Confidence: {:.2}", 
        ocr_result.vendor, ocr_result.amount, ocr_result.confidence_score);

    // Update Metadata
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("ProcessingStatus".to_string(), "Completed".to_string());
    metadata.insert("Confidence".to_string(), format!("{:.2}", ocr_result.confidence_score));
    metadata.insert("ProcessedAt".to_string(), chrono::Utc::now().to_rfc3339());

    if let Some(vendor) = ocr_result.vendor {
        metadata.insert("ProviderName".to_string(), vendor);
    }
    if let Some(amount) = ocr_result.amount {
        metadata.insert("Amount".to_string(), format!("{:.2}", amount));
    }
    if let Some(date) = ocr_result.date {
        metadata.insert("ServiceDate".to_string(), date);
    }
    
    storage.update_metadata("receipts", blob_name, metadata).await?;
    println!("Metadata updated for {}", blob_name);

    // Delete from Queue
    storage.delete_message("receipt-requests", &msg.id, &msg.pop_receipt).await?;
    println!("Message {} deleted successfully.", msg.id);

    Ok(())
}
