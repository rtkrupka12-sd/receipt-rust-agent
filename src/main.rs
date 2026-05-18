mod error;
mod config;
mod processor;

use config::AzureConfig;
use processor::AzureClient;
use processor::azure_queue::{QueueManager, QueueMessage};
use processor::azure_container::BlobManager;
use processor::ocr::ReceiptResult;
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

    println!("Rust Receipt Processor started...");

    loop {
        match storage_client.fetch_message(&config.queue_name).await {
            Ok(Some(msg)) => {
                println!("Received message: {}", msg.id);

                match execute_tiered_workflow(&storage_client, &auditor_client, &config, &config.container_name, msg.clone()).await {
                    Ok(_) => {
                        // Clear message from queue if processing succeeded
                        let _ = storage_client.delete_message(&config.queue_name, &msg.id, &msg.pop_receipt).await;
                    }
                    Err(e) => {
                        eprintln!("Workflow processing failed for message {}: {:?}", msg.id, e);
                        // Message will be visible again in the queue after visibility timeout expires
                    }
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

async fn execute_tiered_workflow<A: AuditEngine>(storage: &AzureClient, auditor: &A, config: &AzureConfig, container_name: &str, msg: QueueMessage) -> Result<(), crate::error::ProcessorError> {
    let blob_name: String = parse_blob_name(&msg.body)?;

    // Download the blob
    println!("Downloading blob: {}", blob_name);
    let image_bytes: Vec<u8> = storage.download_blob(container_name, &blob_name).await?;
    println!("Downloaded {} bytes", image_bytes.len());

    // --- TIER 1: Local Tesseract OCR (with Image Pre-processing) ---
    println!("Executing Tier 1: Local Tesseract Parsing...");
    let local_ocr = TesseractClient::new();
    let mut final_result: ReceiptResult = local_ocr.process_receipt(image_bytes.clone()).await?;

    // --- TIER 2: OpenAI AI Text Audit ---
    if final_result.confidence_score < 0.70 {
        println!("Tier 1 confidence low ({:.2}). Escalating to Tier 2: OpenAI Semantic Audit...", final_result.confidence_score);
        
        let context_text = format!(
            "Line elements found: Vendor Guess: {:?}, Price Guess: {:?}", 
            final_result.vendor, final_result.amount
        );

        match auditor.enrich_result(&context_text, final_result.clone()).await {
            Ok(enriched) => {
                final_result = enriched;
                println!("Tier 2 Audit Complete. Updated Confidence Score: {:.2}", final_result.confidence_score);
            }
            Err(e) => {
                eprintln!("Tier 2 AI Auditor encountered an operational error: {:?}", e);
                // Non-fatal, preserve current Tesseract results for the next threshold check
            }
        }
    }

    // --- TIER 3: Azure Document Intelligence Fallback ---
    if final_result.confidence_score < 0.70 || !final_result.is_verified {
        println!("Tier 2 validation failed or remained low confidence ({:.2}). Escalating to Tier 3: Azure Document Intelligence...", final_result.confidence_score);
        
        let cloud_ocr = DocIntelClient::new(
            config.doc_intel_endpoint.clone(), 
            config.doc_intel_key.clone()
        );
        
        match cloud_ocr.process_receipt(image_bytes).await {
            Ok(cloud_result) => {
                final_result = cloud_result;
                // Cloud OCR results are structurally verified by default assuming successful Azure API parsing
                final_result.is_verified = true; 
                println!("Tier 3 Extraction Successful. Final Vendor: {:?}", final_result.vendor);
            }
            Err(e) => {
                return Err(crate::error::ProcessorError::StorageError(format!(
                    "Critical Failure: All processing tiers exhausted. Cloud OCR failed: {}", e
                )));
            }
        }
    }

    // Image Metadata Update
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("ProcessingStatus".to_string(), "Completed".to_string());

    let normalized_score = (final_result.confidence_score * 100.0).round() / 100.0;
    metadata.insert("Confidence".to_string(), normalized_score.to_string());
    metadata.insert("IsVerified".to_string(), final_result.is_verified.to_string());
    metadata.insert("ProcessedAt".to_string(), chrono::Utc::now().to_rfc3339());

    if let Some(vendor) = final_result.vendor {metadata.insert("ProviderName".to_string(), vendor);}
    if let Some(amount) = final_result.amount {metadata.insert("Amount".to_string(), format!("{:.2}", amount));}
    if let Some(date) = final_result.date {metadata.insert("ServiceDate".to_string(), date);}
    
    storage.update_metadata(container_name, &blob_name, metadata).await?;
    println!("Metadata updated for {}", blob_name);

    Ok(())
}

// Handles multiple message formats from queue
fn parse_blob_name(message_body: &str) -> Result<String, crate::error::ProcessorError> {
    let trimmed = message_body.trim();
    if trimmed.is_empty() {
        return Err(crate::error::ProcessorError::StorageError("Queue message body is empty".into()));
    }
    
    // Handles cases in which message body contains wrapper JSON or file extension quotes
    let clean_name = trimmed
        .trim_matches('"')
        .trim_start_matches("{ \"blob_name\": \"")
        .trim_end_matches("\" }")
        .to_string();

    Ok(clean_name)
    }