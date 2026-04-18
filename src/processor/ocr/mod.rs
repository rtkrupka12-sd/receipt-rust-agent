pub mod doc_intel;

use async_trait::async_trait;
use crate::error::ProcessorError;

#[async_trait]
pub trait OcrEngine {
    // Takes in the bytes of an image and returns a structured ReceiptResult or ProcessorError if processing fails.
    async fn process_receipt(&self, image_bytes: Vec<u8>) -> Result<ReceiptResult, ProcessorError>;
}

pub struct ReceiptResult {
    pub vendor: Option<String>,
    pub amount: Option<f64>,
    pub date: Option<String>,
    pub confidence_score: f32, // Determines if document intelligence backup is needed after OCR processing
}