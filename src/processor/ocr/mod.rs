pub mod doc_intel;
pub mod tesseract;

use async_trait::async_trait;
use crate::error::ProcessorError;

#[async_trait]
pub trait OcrEngine {
    // Takes in the bytes of an image and returns a structured ReceiptResult or ProcessorError if processing fails.
    async fn process_receipt(&self, image_bytes: Vec<u8>) -> Result<ReceiptResult, ProcessorError>;
}

#[derive(Debug, Clone)]
pub struct ReceiptResult {
    pub vendor: Option<String>,
    pub amount: Option<f64>,
    pub date: Option<String>,
    pub category: Option<String>,
    pub confidence_score: f32,
    pub is_verified: bool,
}