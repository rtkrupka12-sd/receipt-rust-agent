mod error;
mod config;
mod processor;

use config::AzureConfig;
use processor::AzureClient;
use processor::azure_queue::{QueueManager, QueueMessage};
use processor::azure_container::BlobManager;
use processor::ocr::OcrEngine;
use processor::ocr::tesseract::TesseractClient;
use processor::ocr::doc_intel::DocIntelClient;
use processor::auditor::{AuditEngine, openai_auditor::OpenAiAuditor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = AzureConfig::from_env()?;
    let storage_client = AzureClient::new(config.clone());

    let openai_key = std::env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY environment variable must be set");
    let auditor_client = OpenAiAuditor::new(openai_key);

    let use_local_ocr = true;

    let queue_name = "receipt-requests"; // TODO: Make this configurable via env var

    println!("Rust Receipt Processor started...");

    loop {
        match storage_client.fetch_message(queue_name).await {
            Ok(Some(msg)) => {
                println!("Received message: {}", msg.id);

                // Strategy Selection
                if use_local_ocr {
                    let local_ocr = TesseractClient::new();
                    let _ = process_workflow(&storage_client, &local_ocr, &auditor_client, msg).await;
                } else {
                    let cloud_ocr = DocIntelClient::new(
                        config.doc_intel_endpoint.clone(), 
                        config.doc_intel_key.clone()
                    );
                    let _ = process_workflow(&storage_client, &cloud_ocr, &auditor_client, msg).await;
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

async fn process_workflow<O: OcrEngine, A: AuditEngine>(storage: &AzureClient, ocr: &O, auditor: &A, msg: QueueMessage) -> Result<(), crate::error::ProcessorError> {
    // TODO: Add more logic to determine blob name
    let blob_name = &msg.body;

    // Download the blob
    println!("Downloading blob: {}", blob_name);
    let blob_data: Vec<u8> = storage.download_blob("receipts", blob_name).await?;
    println!("Downloaded {} bytes", blob_data.len());

    // 1. OCR Processing
    let engine_type = if std::any::type_name::<O>().contains("Tesseract") { "Local Tesseract" } else { "Azure Doc Intel" };
    println!("Processing with {}...", engine_type);
    
    let mut ocr_result = ocr.process_receipt(blob_data).await?;

    // 2. Auditing and Enrichment
    if ocr_result.confidence_score < 0.70 {
        println!(
            "Low confidence score ({:.2}). Triggering AI Auditor via Semantic Routing...", 
            ocr_result.confidence_score
        );
        
        // We'll pass the raw text output to the auditor. 
        // Note: Make sure your TesseractClient returns the raw engine text inside your framework, 
        // or synthesize a clean text block from the values for the prompt string context.
        let contextual_raw_text = format!(
            "Line elements found: Vendor Guess: {:?}, Price Guess: {:?}", 
            ocr_result.vendor, ocr_result.amount
        );

        match auditor.enrich_result(&contextual_raw_text, ocr_result.clone()).await {
            Ok(enriched_result) => {
                ocr_result = enriched_result;
                println!(
                    "AI Audit Complete. New Confidence: {:.2}, Verified: {}, Category: {:?}",
                    ocr_result.confidence_score, ocr_result.is_verified, ocr_result.category
                );
            }
            Err(e) => {
                eprintln!("AI Auditor failed to enrich text data: {:?}", e);
                // Non-fatal error; fall back to storing raw unverified OCR results
            }
        }
    } else {
        println!("High confidence result achieved directly via OCR engine.");
        ocr_result.is_verified = true;
    }

    // 3. Metadata Update
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("ProcessingStatus".to_string(), "Completed".to_string());

    let normalized_score = (ocr_result.confidence_score * 100.0).round() / 100.0;
    metadata.insert("Confidence".to_string(), normalized_score.to_string());
    metadata.insert("IsVerified".to_string(), ocr_result.is_verified.to_string());
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
