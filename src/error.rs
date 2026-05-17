use thiserror::Error;

// Debug: for console display
// Error: custom error types, #[error] attribute for defining error messages, and #[from] for conversion from other error types (e.g. VarError from env vars)
#[derive(Error, Debug)]
pub enum ProcessorError {
    /// AzureConfig::from_env() Errors
    #[error("Missing or invalid configuration: {0}")]
    ConfigError(#[from] std::env::VarError),

    // AzureClient Errors
    #[error("Storage error: {0}")]
    StorageError(String),

    // Azure Blob Errors
    #[error("Blob error: {0}")]
    BlobError(String),

    // Azure Queue Errors
    #[error("Queue error: {0}")]
    QueueError(String),

    // OCR Errors
    #[error("OCR error: {0}")]
    OcrError(String),

    // Audit Errors
    #[error("Audit error: {0}")]
    AiAuditError(String),
}