#[derive(Debug, Clone)]
pub struct AzureConfig {
    pub storage_connection_string: String,
    pub doc_intel_endpoint: String,
    pub doc_intel_key: String,
    pub queue_name: String,
    pub container_name: String,
}

impl AzureConfig {
    pub fn from_env() -> Result<Self, crate::error::ProcessorError> {
        // Dotenvy will load the .env file for local development. In production, this will have access to the Container App's environmental variables.
        // Skip loading .env file during tests to allow tests to control environment variables
        #[cfg(not(test))]
        let _ = dotenvy::dotenv(); 

        Ok(Self {
            storage_connection_string: std::env::var("AZURE_STORAGE_CONNECTION_STRING")?,
            doc_intel_endpoint: std::env::var("AZURE_DOC_INTEL_ENDPOINT")?,
            doc_intel_key: std::env::var("AZURE_DOC_INTEL_KEY")?,
            queue_name: std::env::var("AZURE_QUEUE_NAME").unwrap_or_else(|_| "receipt-requests".to_string()),
            container_name: std::env::var("AZURE_CONTAINER_NAME").unwrap_or_else(|_| "receipts".to_string()),
        })
    }
}

// Unit tests for AzureConfig::from_env

// Note: These tests manipulate environment variables, so they should be run serially to avoid possible dangling references.
// This possible interference is also why the unsafe block is used to set and remove environment variables,
// as Rust's borrow checker does not allow mutable access to environment variables in a safe way.
// The serial_test crate is used to ensure that tests are run one at a time.

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // Valid environmental variables should allow from_env to return a valid AzureConfig
    #[test]
    #[serial]
    fn test_from_env_success() {
        // Set up test environment variables
        unsafe {
            std::env::set_var("AZURE_STORAGE_CONNECTION_STRING", "test_connection_string");
            std::env::set_var("AZURE_DOC_INTEL_ENDPOINT", "https://test.endpoint.com");
            std::env::set_var("AZURE_DOC_INTEL_KEY", "test_key");
            std::env::set_var("AZURE_QUEUE_NAME", "receipt-requests");
            std::env::set_var("AZURE_CONTAINER_NAME", "receipts");
        }

        // Call from_env and verify it returns a valid AzureConfig
        let config = AzureConfig::from_env();
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.storage_connection_string, "test_connection_string");
        assert_eq!(config.doc_intel_endpoint, "https://test.endpoint.com");
        assert_eq!(config.doc_intel_key, "test_key");
        assert_eq!(config.queue_name, "receipt-requests");
        assert_eq!(config.container_name, "receipts");

        // Clean up
        unsafe {
            std::env::remove_var("AZURE_STORAGE_CONNECTION_STRING");
            std::env::remove_var("AZURE_DOC_INTEL_ENDPOINT");
            std::env::remove_var("AZURE_DOC_INTEL_KEY");
            std::env::remove_var("AZURE_QUEUE_NAME");
            std::env::remove_var("AZURE_CONTAINER_NAME");
        }
    }

    // Missing storage connection string should cause from_env to return an error
    #[test]
    #[serial]
    fn test_from_env_missing_storage_connection_string() {
        // Ensure the variable is not set
        unsafe {
            std::env::remove_var("AZURE_STORAGE_CONNECTION_STRING"); // not set
            std::env::set_var("AZURE_DOC_INTEL_ENDPOINT", "https://test.endpoint.com");
            std::env::set_var("AZURE_DOC_INTEL_KEY", "test_key");
            std::env::set_var("AZURE_QUEUE_NAME", "receipt-requests");
            std::env::set_var("AZURE_CONTAINER_NAME", "receipts");
        }

        // Call from_env and verify it returns an error
        let config = AzureConfig::from_env();
        println!("test_from_env_missing_storage_connection_string - config: {:?}", config);
        assert!(matches!(config, Err(crate::error::ProcessorError::ConfigError(_))));

        unsafe {
            std::env::remove_var("AZURE_DOC_INTEL_ENDPOINT");
            std::env::remove_var("AZURE_DOC_INTEL_KEY");
            std::env::remove_var("AZURE_QUEUE_NAME");
            std::env::remove_var("AZURE_CONTAINER_NAME");
        }
    }

    // Missing doc_intel_endpoint should cause from_env to return an error
    #[test]
    #[serial]
    fn test_from_env_missing_doc_intel_endpoint() {
        unsafe {
            std::env::set_var("AZURE_STORAGE_CONNECTION_STRING", "test_connection_string");
            std::env::remove_var("AZURE_DOC_INTEL_ENDPOINT"); // not set
            std::env::set_var("AZURE_DOC_INTEL_KEY", "test_key");
            std::env::set_var("AZURE_QUEUE_NAME", "receipt-requests");
            std::env::set_var("AZURE_CONTAINER_NAME", "receipts");
        }

        let config = AzureConfig::from_env();
        println!("test_from_env_missing_doc_intel_endpoint - config: {:?}", config);
        assert!(matches!(config, Err(crate::error::ProcessorError::ConfigError(_))));

        unsafe {
            std::env::remove_var("AZURE_STORAGE_CONNECTION_STRING");
            std::env::remove_var("AZURE_DOC_INTEL_KEY");
            std::env::remove_var("AZURE_QUEUE_NAME");
            std::env::remove_var("AZURE_CONTAINER_NAME");
        }
    }

    // Missing doc_intel_key should cause from_env to return an error
    #[test]
    #[serial]
    fn test_from_env_missing_doc_intel_key() {
        unsafe {
            std::env::set_var("AZURE_STORAGE_CONNECTION_STRING", "test_connection_string");
            std::env::set_var("AZURE_DOC_INTEL_ENDPOINT", "https://test.endpoint.com");
            std::env::remove_var("AZURE_DOC_INTEL_KEY"); // not set
            std::env::set_var("AZURE_QUEUE_NAME", "receipt-requests");
            std::env::set_var("AZURE_CONTAINER_NAME", "receipts");
        }

        let config = AzureConfig::from_env();
        println!("test_from_env_missing_doc_intel_key - config: {:?}", config);
        assert!(matches!(config, Err(crate::error::ProcessorError::ConfigError(_))));

        unsafe {
            std::env::remove_var("AZURE_STORAGE_CONNECTION_STRING");
            std::env::remove_var("AZURE_DOC_INTEL_ENDPOINT");
            std::env::remove_var("AZURE_QUEUE_NAME");
            std::env::remove_var("AZURE_CONTAINER_NAME");
        }
    }

    // Missing queue name should default to "receipt-requests"
    #[test]
    #[serial]
    fn test_from_env_missing_queue_name() {
        unsafe {
            std::env::set_var("AZURE_STORAGE_CONNECTION_STRING", "test_connection_string");
            std::env::set_var("AZURE_DOC_INTEL_ENDPOINT", "https://test.endpoint.com");
            std::env::set_var("AZURE_DOC_INTEL_KEY", "test_key");
            std::env::remove_var("AZURE_QUEUE_NAME"); // not set
            std::env::set_var("AZURE_CONTAINER_NAME", "receipts");
        }

        let config = AzureConfig::from_env();
        println!("test_from_env_missing_queue_name - config: {:?}", config);
        assert!(matches!(config, Ok(cfg) if cfg.queue_name == "receipt-requests"));

        unsafe {
            std::env::remove_var("AZURE_STORAGE_CONNECTION_STRING");
            std::env::remove_var("AZURE_DOC_INTEL_ENDPOINT");
            std::env::remove_var("AZURE_DOC_INTEL_KEY");
            std::env::remove_var("AZURE_QUEUE_NAME");
            std::env::remove_var("AZURE_CONTAINER_NAME");
        }
    }

    // Missing container name should default to "receipts"
    #[test]
    #[serial]
    fn test_from_env_missing_container_name() {
        unsafe {
            std::env::set_var("AZURE_STORAGE_CONNECTION_STRING", "test_connection_string");
            std::env::set_var("AZURE_DOC_INTEL_ENDPOINT", "https://test.endpoint.com");
            std::env::set_var("AZURE_DOC_INTEL_KEY", "test_key");
            std::env::set_var("AZURE_QUEUE_NAME", "receipt-requests");
            std::env::remove_var("AZURE_CONTAINER_NAME"); // not set
        }

        let config = AzureConfig::from_env();
        println!("test_from_env_missing_container_name - config: {:?}", config);
        assert!(matches!(config, Ok(cfg) if cfg.container_name == "receipts"));

        unsafe {
            std::env::remove_var("AZURE_STORAGE_CONNECTION_STRING");
            std::env::remove_var("AZURE_DOC_INTEL_ENDPOINT");
            std::env::remove_var("AZURE_DOC_INTEL_KEY");
            std::env::remove_var("AZURE_QUEUE_NAME");
            std::env::remove_var("AZURE_CONTAINER_NAME");
        }
    }
}
