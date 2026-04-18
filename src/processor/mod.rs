// makes processor module public and accessible from main.rs
pub mod azure_queue;
pub mod azure_container;

use crate::config::AzureConfig;
use crate::error::ProcessorError;
use azure_storage::StorageCredentials;

pub struct AzureClient {
    pub config: AzureConfig,
}

impl AzureClient {
    pub fn new(config: AzureConfig) -> Self {
        Self { config }
    }

    pub fn get_credentials(&self) -> Result<(String, StorageCredentials), ProcessorError> {
        let conn_str: &str = &self.config.storage_connection_string;
        let parts: std::collections::HashMap<&str, &str> = conn_str
            .split(';')
            .filter_map(|s| s.split_once('='))
            .collect();

        let account: &str = parts.get("AccountName")
            .ok_or_else(|| ProcessorError::StorageError("Missing AccountName".into()))?;
        let key: &str = parts.get("AccountKey")
            .ok_or_else(|| ProcessorError::StorageError("Missing AccountKey".into()))?;

        let credentials: StorageCredentials = StorageCredentials::access_key(account.to_string(), key.to_string());
        Ok((account.to_string(), credentials))
    }
}