pub mod openai_auditor;

use async_trait::async_trait;
use crate::error::ProcessorError;
use crate::processor::ocr::ReceiptResult;

#[async_trait]
pub trait AuditEngine {
    async fn enrich_result(
        &self, 
        raw_text: &str, 
        current_result: ReceiptResult
    ) -> Result<ReceiptResult, ProcessorError>;
}