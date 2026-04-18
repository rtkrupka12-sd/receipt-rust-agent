use mockall::automock;
use async_trait::async_trait;
use crate::error::ProcessorError;
use super::AzureClient;
use azure_storage_queues::prelude::*;

#[automock] // macro from mockall to generate a mock implementation of the QueueManager trait for testing purposes
#[async_trait] // macro from async_trait to allow async functions in traits

pub trait QueueManager {
    // Checks the queue for one message.
    async fn fetch_message(&self, queue_name: &str) -> Result<Option<QueueMessage>, ProcessorError>;
    // Deletes a message from the queue using its ID and pop receipt
    async fn delete_message(&self, queue_name: &str, message_id: &str, pop_receipt: &str) -> Result<(), ProcessorError>;
}

#[async_trait]
impl QueueManager for AzureClient {
    async fn fetch_message(&self, queue_name: &str) -> Result<Option<QueueMessage>, ProcessorError> {
        let (account, credentials) = self.get_credentials()?;
        let queue_service: QueueServiceClient = QueueServiceClient::new(account, credentials);
        let queue_client: QueueClient = queue_service.queue_client(queue_name);

        // Try to get a message
        // Visibility timeout of 30s to reserve the message
        let response = queue_client
            .get_messages()
            .number_of_messages(1)
            .visibility_timeout(std::time::Duration::from_secs(30))
            .into_future()
            .await
            .map_err(|e: azure_core::Error| ProcessorError::QueueError(e.to_string()))?;

        // Convert Azure SDK message to our QueueMessage type
        let azure_msg = response.messages.into_iter().next();
        Ok(azure_msg.map(|msg| QueueMessage {
            id: msg.message_id,
            pop_receipt: msg.pop_receipt,
            body: msg.message_text,
        }))
    }

    async fn delete_message(&self, queue_name: &str, message_id: &str, pop_receipt: &str) -> Result<(), ProcessorError> {
        let (account, credentials) = self.get_credentials()?;
        let queue_service: QueueServiceClient = QueueServiceClient::new(account, credentials);
        let queue_client: QueueClient = queue_service.queue_client(queue_name);

        let pop_receipt_obj: PopReceipt = PopReceipt::new(message_id.to_string(), pop_receipt.to_string());

        queue_client
            .pop_receipt_client(pop_receipt_obj)
            .delete()
            .await
            .map_err(|e: azure_core::Error| ProcessorError::QueueError(e.to_string()))?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct QueueMessage {
    pub id: String,
    pub pop_receipt: String, // Fetching a message just hides it during a visibility timeout, so we need the pop receipt to delete it after processing
    pub body: String,
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

    // Config creation and QueueMessage struct

    // AzureClient creation with config should contain the correct connection string
    #[test]
    fn test_azure_client_creation() {
        let config = create_test_config();
        let client = AzureClient::new(config);

        assert_eq!(
            client.config.storage_connection_string,
            "DefaultEndpointsProtocol=https;AccountName=testaccount;AccountKey=dGVzdGtleQ==;EndpointSuffix=core.windows.net"
        );
    }

    // QueueMessage struct should contain the correct id, pop_receipt, and body
    #[test]
    fn test_queue_message_creation() {
        let message = QueueMessage {
            id: "test-id".to_string(),
            pop_receipt: "test-receipt".to_string(),
            body: "test body content".to_string(),
        };

        assert_eq!(message.id, "test-id");
        assert_eq!(message.pop_receipt, "test-receipt");
        assert_eq!(message.body, "test body content");
    }

    // Using Mockall for fetching and deleting messages

    // fetch_message should return a message when the queue has messages
    #[tokio::test]
    async fn test_fetch_message_with_mock_returns_some_message() {
        // Arrange
        let mut mock = MockQueueManager::new();
        
        mock.expect_fetch_message()
            .with(mockall::predicate::eq("test-queue"))
            .times(1)
            .returning(|_| {
                Ok(Some(QueueMessage {
                    id: "msg-123".to_string(),
                    pop_receipt: "receipt-456".to_string(),
                    body: "Hello from queue".to_string(),
                }))
            });
        
        // Act
        let result = mock.fetch_message("test-queue").await;

        // Assert
        assert!(result.is_ok());
        let message = result.unwrap();
        assert!(message.is_some());

        let msg = message.unwrap();
        assert_eq!(msg.id, "msg-123");
        assert_eq!(msg.pop_receipt, "receipt-456");
        assert_eq!(msg.body, "Hello from queue");
    }

    // fetch_message should return None when the queue is empty
    #[tokio::test]
    async fn test_fetch_message_with_mock_returns_none() {
        // Arrange
        let mut mock = MockQueueManager::new();
        mock.expect_fetch_message()
            .with(mockall::predicate::eq("empty-queue"))
            .times(1)
            .returning(|_| Ok(None));

        // Act
        let result = mock.fetch_message("empty-queue").await;

        // Assert
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // fetch_message should return an error when there is a queue issue
    #[tokio::test]
    async fn test_fetch_message_with_mock_returns_error() {
        // Arrange
        let mut mock = MockQueueManager::new();
        mock.expect_fetch_message()
            .with(mockall::predicate::eq("error-queue"))
            .times(1)
            .returning(|_| Err(ProcessorError::QueueError("Queue not found".to_string())));

        // Act
        let result = mock.fetch_message("error-queue").await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProcessorError::QueueError(_)));
    }

    // delete_message should succeed when given valid parameters
    #[tokio::test]
    async fn test_delete_message_with_mock_success() {
        
        // Arrange
        let mut mock = MockQueueManager::new();
        mock.expect_delete_message()
            .with(
                mockall::predicate::eq("test-queue"),
                mockall::predicate::eq("msg-123"),
                mockall::predicate::eq("receipt-456")
            )
            .times(1)
            .returning(|_, _, _| Ok(()));

        // Act
        let result = mock.delete_message("test-queue", "msg-123", "receipt-456").await;

        // Assert
        assert!(result.is_ok());
    }

    // delete_message should return an error when given invalid parameters
    #[tokio::test]
    async fn test_delete_message_with_mock_returns_error() {
        // Arrange
        let mut mock = MockQueueManager::new();
        mock.expect_delete_message()
            .with(
                mockall::predicate::eq("test-queue"),
                mockall::predicate::eq("invalid-id"),
                mockall::predicate::eq("invalid-receipt")
            )
            .times(1)
            .returning(|_, _, _| Err(ProcessorError::QueueError("Message not found".to_string())));

        // Act
        let result = mock.delete_message("test-queue", "invalid-id", "invalid-receipt").await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProcessorError::QueueError(_)));
    }

    // Simulating a full message processing workflow: fetch a message, process it, and then delete it
    #[tokio::test]
    async fn test_message_processing_workflow_with_mock() {
        // Arrange
        let mut mock = MockQueueManager::new();
        mock.expect_fetch_message()
            .with(mockall::predicate::eq("workflow-queue"))
            .times(1)
            .returning(|_| {
                Ok(Some(QueueMessage {
                    id: "workflow-msg".to_string(),
                    pop_receipt: "workflow-receipt".to_string(),
                    body: "Process this".to_string(),
                }))
            });

        mock.expect_delete_message()
            .with(
                mockall::predicate::eq("workflow-queue"),
                mockall::predicate::eq("workflow-msg"),
                mockall::predicate::eq("workflow-receipt")
            )
            .times(1)
            .returning(|_, _, _| Ok(()));

        // Act
        let fetch_result = mock.fetch_message("workflow-queue").await; // Fetch the message
        // Assert
        assert!(fetch_result.is_ok());

        let message = fetch_result.unwrap(); // Unwrap the fetch result to get the message
        assert!(message.is_some());

        let msg = message.unwrap(); // Unwrap the Option to get the actual message
        assert_eq!(msg.body, "Process this");

        let delete_result = mock.delete_message("workflow-queue", &msg.id, &msg.pop_receipt).await; // Delete the message after processing
        assert!(delete_result.is_ok());
    }
}

