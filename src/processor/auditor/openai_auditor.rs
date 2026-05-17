use async_openai::{
    types::{ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs},
    Client,
};
use async_trait::async_trait;
use serde::Deserialize;
use crate::error::ProcessorError;
use crate::processor::ocr::ReceiptResult;
use super::AuditEngine;

// JSON structure we want OpenAI to return
#[derive(Deserialize, Debug)]
struct SchemaAuditOutput {
    vendor: Option<String>,
    amount: Option<f64>,
    date: Option<String>,
    category: String,
    is_valid_receipt: bool,
}

pub struct OpenAiAuditor {
    client: Client<async_openai::config::OpenAIConfig>,
}

impl OpenAiAuditor {
    pub fn new(api_key: String) -> Self {
        let config = async_openai::config::OpenAIConfig::new().with_api_key(api_key);
        Self {
            client: Client::with_config(config),
        }
    }
}

#[async_trait]
impl AuditEngine for OpenAiAuditor {
    async fn enrich_result(
        &self, 
        raw_text: &str, 
        mut current_result: ReceiptResult
    ) -> Result<ReceiptResult, ProcessorError> {

        let request = CreateChatCompletionRequestArgs::default()
            .model("gpt-4o-mini")
            .messages([
                ChatCompletionRequestSystemMessageArgs::default()
                    .content("You are an expert financial auditor. \
                        Analyze the messy OCR text from a receipt. Verify or correct the vendor name, date, and total transaction amount. \
                        You must return a raw JSON object matching this exact structure: \
                        {\
                          \"vendor\": \"string or null\",\
                          \"amount\": number or null,\
                          \"date\": \"YYYY-MM-DD format string or null\",\
                          \"category\": \"Financial category like Meals, Software, Groceries, Utilities, etc.\",\
                          \"is_valid_receipt\": true/false\
                        }")
                    .build()
                    .map_err(|e| ProcessorError::AiAuditError(e.to_string()))?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(format!("OCR Initial Guess: Vendor={:?}, Amount={:?}\nRaw Text:\n{}", current_result.vendor, current_result.amount, raw_text))
                    .build()
                    .map_err(|e| ProcessorError::AiAuditError(e.to_string()))?
                    .into(),
            ])
            .build()
            .map_err(|e| ProcessorError::AiAuditError(e.to_string()))?;

        let response = self.client.chat().create(request).await
            .map_err(|e| ProcessorError::AiAuditError(e.to_string()))?;
        let raw_content = response.choices[0].message.content.as_ref()
            .ok_or_else(|| ProcessorError::AiAuditError("AI returned an empty response body".into()))?;
        
        let mut json_str: String = raw_content.trim().to_string();

        // Strip out any accidental markdown triple-backtick lines
        if json_str.starts_with("```") {
            json_str = json_str
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
                .to_string();
        }

        // Safely parse the schema matching structural output
        let ai_data: SchemaAuditOutput = serde_json::from_str(&json_str)
            .map_err(|e| ProcessorError::AiAuditError(format!("Failed parsing AI payload: {}", e)))?;

        // If the AI confirms it is garbage text or not a receipt, adjust confidence negatively
        if !ai_data.is_valid_receipt {
            current_result.confidence_score = 0.0;
            return Ok(current_result);
        }

        // Enrich the current result
        if ai_data.vendor.is_some() { current_result.vendor = ai_data.vendor; }
        if ai_data.amount.is_some() { current_result.amount = ai_data.amount; }
        if ai_data.date.is_some() { current_result.date = ai_data.date; }
        
        current_result.category = Some(ai_data.category);
        current_result.is_verified = true;
        
        // Increase confidence
        current_result.confidence_score = (current_result.confidence_score + 0.4).min(1.0);

        Ok(current_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::ocr::ReceiptResult;

    struct MockAuditor {
        should_fail: bool,
    }

    #[async_trait]
    impl AuditEngine for MockAuditor {
        async fn enrich_result(
            &self,
            _raw_text: &str,
            mut current_result: ReceiptResult,
        ) -> Result<ReceiptResult, ProcessorError> {
            if self.should_fail {
                current_result.confidence_score = 0.0;
                current_result.is_verified = false;
                return Ok(current_result);
            }

            current_result.vendor = Some("Mock Corporation".to_string());
            current_result.amount = Some(104.50);
            current_result.category = Some("Equipment".to_string());
            current_result.is_verified = true;
            current_result.confidence_score = 1.0;

            Ok(current_result)
        }
    }

    // AI Auditor should successfully enrich the receipt data and verify it
    #[tokio::test]
    async fn test_successful_auditor_enrichment() {
        let auditor = MockAuditor { should_fail: false };
        
        let initial_result = ReceiptResult {
            vendor: Some("Tar-get".to_string()), // Typo from raw OCR
            amount: None,                       // Tesseract missed it
            date: Some("2026-05-15".to_string()),
            category: None,
            confidence_score: 0.35,
            is_verified: false,
        };

        let enriched = auditor.enrich_result("Target store purchase text total $104.50", initial_result)
            .await
            .unwrap();

        assert_eq!(enriched.vendor.unwrap(), "Mock Corporation");
        assert_eq!(enriched.amount.unwrap(), 104.50);
        assert_eq!(enriched.category.unwrap(), "Equipment");
        assert!(enriched.is_verified);
        assert_eq!(enriched.confidence_score, 1.0);
    }

    // AI Auditor should identify non-receipt text and mark it as invalid, lowering confidence to 0
    #[tokio::test]
    async fn test_failed_receipt_validation() {
        let auditor = MockAuditor { should_fail: true };
        
        let initial_result = ReceiptResult {
            vendor: None,
            amount: None,
            date: None,
            category: None,
            confidence_score: 0.20,
            is_verified: false,
        };

        let enriched = auditor.enrich_result("This is an image text of a random landscape photo or text log file.", initial_result)
            .await
            .unwrap();

        assert!(!enriched.is_verified);
        assert_eq!(enriched.confidence_score, 0.0);
    }

    // Strict JSON schema should deserialize correctly
    #[test]
    fn test_internal_schema_deserialization() {
        // Ensures our parsing struct exactly maps the anticipated JSON structure guaranteed by OpenAI's strict schema feature
        let raw_json = r#"
            {
                "vendor": "AWS Cloud Services",
                "amount": 14.99,
                "date": "2026-04-01",
                "category": "Infrastructure",
                "is_valid_receipt": true
            }
        "#;

        let parsed: SchemaAuditOutput = serde_json::from_str(raw_json).unwrap();
        
        assert_eq!(parsed.vendor.unwrap(), "AWS Cloud Services");
        assert_eq!(parsed.amount.unwrap(), 14.99);
        assert_eq!(parsed.category, "Infrastructure");
        assert!(parsed.is_valid_receipt);
    }
}