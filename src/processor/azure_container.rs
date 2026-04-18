use mockall::automock;
use async_trait::async_trait;
use crate::error::ProcessorError;
use super::AzureClient;
use azure_storage_blobs::prelude::*;
use futures::StreamExt;

#[automock]
#[async_trait]

pub trait BlobManager {
    // Downloads a blob from the specified container and blob name, and returns a byte vector if successful and ProcessorError if failure occurs.
    async fn download_blob(&self, container: &str, blob_name: &str) -> Result<Vec<u8>, ProcessorError>;
    // Updates metadata for a blob in the specified container. Metadata values are sanitized to ensure ASCII compatibility.
    async fn update_metadata(&self, container: &str, blob_name: &str, metadata: std::collections::HashMap<String, String>) -> Result<(), ProcessorError>;
}

#[async_trait]
impl BlobManager for AzureClient {
    async fn download_blob(&self, container_name: &str, blob_name: &str) -> Result<Vec<u8>, ProcessorError> {
        let (account, creds): (String, azure_storage::StorageCredentials) = self.get_credentials()?;
        let blob_service: BlobServiceClient = BlobServiceClient::new(account, creds);
        let container_client: ContainerClient = blob_service.container_client(container_name);
        let blob_client: BlobClient = container_client.blob_client(blob_name);

        let mut data: Vec<u8> = Vec::new();
        let mut stream = blob_client
            .get()
            .into_stream();

        // Collect the stream of bytes into a vector. Each chunk is processed as it arrives.
        while let Some(chunk_result) = stream.next().await {
            let response = chunk_result.map_err(|e: azure_core::Error| ProcessorError::BlobError(e.to_string()))?;
            let body_bytes = response.data.collect().await.map_err(|e: azure_core::Error| ProcessorError::BlobError(e.to_string()))?;
            data.extend_from_slice(&body_bytes);
        }

        Ok(data)
    }

    async fn update_metadata(&self, container_name: &str, blob_name: &str, metadata: std::collections::HashMap<String, String>
        ) -> Result<(), ProcessorError> {
        let (account, creds): (String, azure_storage::StorageCredentials) = self.get_credentials()?;
        let blob_service: BlobServiceClient = BlobServiceClient::new(account, creds);
        let blob_client: BlobClient = blob_service.container_client(container_name).blob_client(blob_name);

        // Create Metadata object and populate it with sanitized values
        let mut blob_metadata: azure_core::prelude::Metadata = azure_core::prelude::Metadata::new();

        for (key, value) in metadata {
            // Clean metadata values to ensure they are ASCII and do not contain control characters as they are not compatible with Azure metadata
            let sanitized_value: String = value.chars()
                .filter(|c| c.is_ascii() && !c.is_control())
                .collect::<String>();

            blob_metadata.insert(key, sanitized_value);
        }

        blob_client
            .set_metadata()
            .metadata(blob_metadata)
            .into_future()
            .await
            .map_err(|e: azure_core::Error| ProcessorError::BlobError(format!("Metadata update failed: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AzureConfig;

    fn create_test_config() -> AzureConfig {
        AzureConfig {
            storage_connection_string: "DefaultEndpointsProtocol=https;AccountName=testaccount;AccountKey=dGVzdGtleQ==;EndpointSuffix=core.windows.net".to_string(),
            doc_intel_endpoint: "https://test.cognitiveservices.azure.com/".to_string(),
            doc_intel_key: "test_key".to_string(),
        }
    }

    // AzureClient creation with config should contain the correct connection string
    #[test]
    fn test_azure_client_creation() {
        let config: AzureConfig = create_test_config();
        let client: AzureClient = AzureClient::new(config);

        assert_eq!(
            client.config.storage_connection_string,
            "DefaultEndpointsProtocol=https;AccountName=testaccount;AccountKey=dGVzdGtleQ==;EndpointSuffix=core.windows.net"
        );
    }

    // Using Mockall for downloading blobs and updating metadata

    // download_blob should return blob data when the blob exists
    #[tokio::test]
    async fn test_download_blob_with_mock_returns_data() {
        // Arrange
        let mut mock: MockBlobManager = MockBlobManager::new();

        mock.expect_download_blob()
            .with(
                mockall::predicate::eq("test-container"),
                mockall::predicate::eq("test-blob.txt")
            )
            .times(1)
            .returning(|_, _| {
                Ok(vec![72, 101, 108, 108, 111]) // "Hello" in bytes
            });

        // Act
        let result = mock.download_blob("test-container", "test-blob.txt").await;

        // Assert
        assert!(result.is_ok());
        let data: Vec<u8> = result.unwrap();
        assert_eq!(data, vec![72, 101, 108, 108, 111]);
        assert_eq!(String::from_utf8(data).unwrap(), "Hello");
    }

    // download_blob should return an error when the blob does not exist
    #[tokio::test]
    async fn test_download_blob_with_mock_returns_error() {
        // Arrange
        let mut mock: MockBlobManager = MockBlobManager::new();
        mock.expect_download_blob()
            .with(
                mockall::predicate::eq("test-container"),
                mockall::predicate::eq("nonexistent-blob.txt")
            )
            .times(1)
            .returning(|_, _| Err(ProcessorError::BlobError("Blob not found".to_string())));

        // Act
        let result = mock.download_blob("test-container", "nonexistent-blob.txt").await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProcessorError::BlobError(_)));
    }

    // update_metadata should succeed when given valid parameters
    #[tokio::test]
    async fn test_update_metadata_with_mock_success() {
        // Arrange
        let mut mock: MockBlobManager = MockBlobManager::new();
        let mut metadata: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        metadata.insert("status".to_string(), "processed".to_string());
        metadata.insert("version".to_string(), "1.0".to_string());

        mock.expect_update_metadata()
            .with(
                mockall::predicate::eq("test-container"),
                mockall::predicate::eq("test-blob.txt"),
                mockall::predicate::eq(metadata.clone())
            )
            .times(1)
            .returning(|_, _, _| Ok(()));

        // Act
        let result = mock.update_metadata("test-container", "test-blob.txt", metadata).await;

        // Assert
        assert!(result.is_ok());
    }

    // update_metadata should return an error when update fails
    #[tokio::test]
    async fn test_update_metadata_with_mock_returns_error() {
        // Arrange
        let mut mock: MockBlobManager = MockBlobManager::new();
        let mut metadata: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        metadata.insert("status".to_string(), "failed".to_string());

        mock.expect_update_metadata()
            .with(
                mockall::predicate::eq("test-container"),
                mockall::predicate::eq("failed-blob.txt"),
                mockall::predicate::eq(metadata.clone())
            )
            .times(1)
            .returning(|_, _, _| Err(ProcessorError::BlobError("Metadata update failed".to_string())));

        // Act
        let result = mock.update_metadata("test-container", "failed-blob.txt", metadata).await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProcessorError::BlobError(_)));
    }

    // Simulating a full blob processing workflow: download a blob, process it, and update metadata
    #[tokio::test]
    async fn test_blob_processing_workflow_with_mock() {
        // Arrange
        let mut mock: MockBlobManager = MockBlobManager::new();
        let original_data: Vec<u8> = vec![72, 101, 108, 108, 111]; // "Hello"
        let mut metadata: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        metadata.insert("status".to_string(), "completed".to_string());
        metadata.insert("processed_at".to_string(), "2024-01-01T00:00:00Z".to_string());

        mock.expect_download_blob()
            .with(
                mockall::predicate::eq("input-container"),
                mockall::predicate::eq("input-blob.txt")
            )
            .times(1)
            .returning(move |_, _| Ok(original_data.clone()));

        mock.expect_update_metadata()
            .with(
                mockall::predicate::eq("input-container"),
                mockall::predicate::eq("input-blob.txt"),
                mockall::predicate::eq(metadata.clone())
            )
            .times(1)
            .returning(|_, _, _| Ok(()));

        // Act
        let download_result = mock.download_blob("input-container", "input-blob.txt").await;

        // Assert download
        assert!(download_result.is_ok());

        let downloaded_data: Vec<u8> = download_result.unwrap();
        assert_eq!(downloaded_data, vec![72, 101, 108, 108, 111]);

        // Simulate processing and update metadata
        let update_result = mock.update_metadata("input-container", "input-blob.txt", metadata).await;

        // Assert metadata update
        assert!(update_result.is_ok());
    }
}