use async_trait::async_trait;
use std::time::Duration;
use tokio::time::sleep;

use super::{OcrEngine, ReceiptResult};
use crate::error::ProcessorError;

pub struct DocIntelClient {
    endpoint: String,
    key: String,
    client: reqwest::Client,
}

impl DocIntelClient {
    pub fn new(endpoint: String, key: String) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl OcrEngine for DocIntelClient {
    async fn process_receipt(&self, image_bytes: Vec<u8>) -> Result<ReceiptResult, ProcessorError> {
        // Submission - Polling - Retrieval pattern for Azure Document Intelligence API    

        // Submission of the image to the API for analysis
        let url: String = format!("{}/documentintelligence/documentModels/prebuilt-receipt:analyze?api-version=2024-02-29-preview", self.endpoint);
        let response: reqwest::Response = self.client.post(&url)
            .header("Ocp-Apim-Subscription-Key", &self.key)
            .header("Content-Type", "application/octet-stream")
            .body(image_bytes)
            .send()
            .await
            .map_err(|e: reqwest::Error| ProcessorError::BlobError(format!("DI request failed: {}", e)))?;

        // Azure DI gives us a polling header after accepting the request
        let operation_url: &str = response.headers()
            .get("Operation-Location")
            .ok_or_else(|| ProcessorError::BlobError("Missing Operation-Location header".into()))?
            .to_str()
            .map_err(|_| ProcessorError::BlobError("Invalid header format".into()))?;

        // Regularly poll the operation URL until we get a success or failure status. If it's still running, wait and poll again.
        loop {
            let poll_resp: reqwest::Response = self.client.get(operation_url)
                .header("Ocp-Apim-Subscription-Key", &self.key)
                .send()
                .await
                .map_err(|e: reqwest::Error| ProcessorError::BlobError(e.to_string()))?;

            let status_json: serde_json::Value = poll_resp.json().await
                .map_err(|e: reqwest::Error| ProcessorError::BlobError(e.to_string()))?;

            match status_json["status"].as_str() {
                Some("succeeded") => {
                    return parse_di_result(status_json);
                }
                Some("failed") => return Err(ProcessorError::BlobError("DI Analysis failed".into())),
                _ => {
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            }
        }
    }
}

// Extract the specific fields from the DI response
fn parse_di_result(json: serde_json::Value) -> Result<ReceiptResult, ProcessorError> {
    let fields: &serde_json::Value = &json["analyzeResult"]["documents"][0]["fields"];

    Ok(ReceiptResult {
        vendor: fields["MerchantName"]["valueString"].as_str().map(|s: &str| s.to_string()),
        amount: fields["Total"]["valueCurrency"]["amount"].as_f64(),
        date: fields["TransactionDate"]["valueDate"].as_str().map(|s: &str| s.to_string()),
        confidence_score: json["analyzeResult"]["documents"][0]["confidence"].as_f64().unwrap_or(0.0) as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde_json::json;

    #[tokio::test]
    async fn test_doc_intel_full_flow_success() {
        let mut server: mockito::ServerGuard = Server::new_async().await;
        let url: String = server.url();

        // Submission Mock
        let post_mock = server.mock("POST", "/documentintelligence/documentModels/prebuilt-receipt:analyze?api-version=2024-02-29-preview")
            .match_header("Ocp-Apim-Subscription-Key", "fake-key")
            .with_status(202)
            .with_header("Operation-Location", &format!("{}/operations/123", url))
            .create_async()
            .await;

        // Mock getting the polling header and receiving a successful analysis result
        let get_mock = server.mock("GET", "/operations/123")
            .with_status(200)
            .with_body(json!({
                "status": "succeeded",
                "analyzeResult": {
                    "documents": [{
                        "confidence": 0.98,
                        "fields": {
                            "MerchantName": { "valueString": "Whole Foods" },
                            "Total": { "valueCurrency": { "amount": 25.50 } },
                            "TransactionDate": { "valueDate": "2024-05-20" }
                        }
                    }]
                }
            }).to_string())
            .create_async()
            .await;

        let client: DocIntelClient = DocIntelClient::new(url, "fake-key".to_string());
        let result: ReceiptResult = client.process_receipt(vec![0, 1, 2]).await.unwrap();

        assert_eq!(result.vendor.unwrap(), "Whole Foods");
        assert_eq!(result.amount.unwrap(), 25.50);
        assert!(result.confidence_score > 0.9);
        
        post_mock.assert_async().await;
        get_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_doc_intel_retries_on_running_status() {
        let mut server: mockito::ServerGuard = Server::new_async().await;
        let url: String = server.url();

        let _post = server.mock("POST", mockito::Matcher::Any)
            .with_status(202)
            .with_header("Operation-Location", &format!("{}/poll", url))
            .create_async().await;

        // Mock getting the polling header and receiving a "running" status first
        let get_running: mockito::Mock = server.mock("GET", "/poll")
            .with_status(200)
            .with_body(json!({"status": "running"}).to_string())
            .expect(1)
            .create_async().await;

        // Mock the next poll returning a success status
        let get_success: mockito::Mock = server.mock("GET", "/poll")
            .with_status(200)
            .with_body(json!({
                "status": "succeeded",
                "analyzeResult": { "documents": [{ "confidence": 1.0, "fields": {} }] }
            }).to_string())
            .expect(1)
            .create_async().await;

        let client: DocIntelClient = DocIntelClient::new(url, "key".to_string());

        let _ = client.process_receipt(vec![0]).await;

        get_running.assert_async().await;
        get_success.assert_async().await;
    }
}