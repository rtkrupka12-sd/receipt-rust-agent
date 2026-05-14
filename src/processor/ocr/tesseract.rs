use async_trait::async_trait;
use crate::processor::ocr::{OcrEngine, ReceiptResult};
use crate::error::ProcessorError;
use regex::Regex;

pub struct TesseractClient;

impl TesseractClient {
    pub fn new() -> Self {
        Self
    }

    // Find the dollar amount (e.g., $12.34 or 12.34)
    // For each match, parse it as a float and keep track of the maximum value found, which is likely the total amount on the receipt.
    fn extract_amount(&self, text: &str) -> Option<f64> {
        let re = Regex::new(r"(\d+\.\d{2})").ok()?;
        re.find_iter(text) // Find all matches in the text
            .filter_map(|m| m.as_str().parse::<f64>().ok()) // Parse each match as a float, ignoring any that fail to parse
            .max_by(|a: &f64, b: &f64| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)) // Treat as equal if parsing fails
    }

    // Vendor name extraction
    // Takes each line of text, trims it, and returns the first non-empty line that is longer than 3 characters, which is likely the vendor name.
    fn extract_vendor(&self, text: &str) -> Option<String> {
        text.lines()
            .map(|l: &str| l.trim())
            .find(|l: &&str| {
                !l.is_empty()
                && l.len() > 3
                && l.len() < 50 // Ignore lines that are too long to be a vendor name
                && l.chars().any(|c: char| c.is_alphabetic())
            })
            .map(|s: &str| s.to_string())
    }
}

#[async_trait] // macro from async_trait to allow async functions in traits
impl OcrEngine for TesseractClient {
    async fn process_receipt(&self, image_data: Vec<u8>) -> Result<ReceiptResult, ProcessorError> {
        // Spawn blocking will run the Tesseract OCR in a separate thread so it doesn't block the async runtime, since Tesseract is CPU-bound and not async-friendly.
        // Step 1: Perform OCR using Tesseract on the image data
        let raw_text: String = tokio::task::spawn_blocking(move || -> Result<String, ProcessorError> {
            // 1. Initialize the Tesseract API
            let mut api = leptess::tesseract::TessApi::new(None, "eng")
                .map_err(|e| ProcessorError::OcrError(format!("Failed to init Tesseract: {}", e)))?;

            // 2. Load the image from memory
            let pix = leptess::leptonica::pix_read_mem(&image_data)
                .map_err(|e| ProcessorError::OcrError(format!("Leptonica error: {}", e)))?;

            // 3. Perform the OCR
            api.set_image(&pix);
            
            // 4. Extract and return the text
            // Note: Removed the ? inside here to keep it clean
            api.get_utf8_text()
                .map_err(|e| ProcessorError::OcrError(format!("OCR error: {}", e)))
        })
        .await
        .map_err(|_| ProcessorError::OcrError("Thread join error".to_string()))??;

        // Step 2: Parse data
        let vendor: Option<String> = self.extract_vendor(&raw_text);
        let amount: Option<f64> = self.extract_amount(&raw_text);
        
        // TODO: Improve confidence score calcuation by considering OCR quality metrics

        // Step 3: Determine Confidence
        // Confidence is based on how many fields were found
        let mut score: f32 = 0.0;
        if vendor.is_some() { score += 0.4; }
        if amount.is_some() { score += 0.4; }
        // Low amount of text is likely due to a bad scan, so modulate Confidence based on text length.
        if raw_text.len() > 50 { score += 0.15; }

        Ok(ReceiptResult {
            vendor,
            amount,
            date: None, // TODO: Add date parsing logic
            confidence_score: score,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Extract Amount should find the largest amount in the text
    #[test]
    fn test_extract_amount_finds_maximum() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "Item 1: $5.99\nItem 2: $12.50\nTotal: $18.49";
        
        let amount: Option<f64> = client.extract_amount(text);
        
        assert_eq!(amount, Some(18.49));
    }

    // Extract Amount should work even if there is no dollar sign, as long as there are valid decimal numbers
    #[test]
    fn test_extract_amount_with_no_dollar_sign() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "Subtotal 45.99\nTax 3.68\nTotal 49.67";
        
        let amount: Option<f64> = client.extract_amount(text);
        
        assert_eq!(amount, Some(49.67));
    }

    // Extract Amount should return None if there are no valid amounts in the text
    #[test]
    fn test_extract_amount_returns_none_when_no_amounts() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "No prices here";
        
        let amount: Option<f64> = client.extract_amount(text);
        
        assert_eq!(amount, None);
    }

    // Extract Amount should ignore invalid formats and still find valid amounts
    #[test]
    fn test_extract_amount_with_multiple_decimals() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "Price: 9.99\nTax: 0.80\nDiscount: -2.50\nTotal: 8.29";
        
        let amount: Option<f64> = client.extract_amount(text);
        
        assert_eq!(amount, Some(9.99));
    }

    // Extract Vendor should return the first long line of text
    #[test]
    fn test_extract_vendor_returns_first_long_line() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "\n  \nWhole Foods Market\n123 Main St\nTotal: $25.50";
        
        let vendor: Option<String> = client.extract_vendor(text);
        
        assert_eq!(vendor, Some("Whole Foods Market".to_string()));
    }

    // Extract Vendor should ignore short lines
    #[test]
    fn test_extract_vendor_ignores_short_lines() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "AB\nXY\nTarget Store\nReceipt";
        
        let vendor: Option<String> = client.extract_vendor(text);
        
        assert_eq!(vendor, Some("Target Store".to_string()));
    }

    // Extract Vendor should return None if there are no valid lines
    #[test]
    fn test_extract_vendor_returns_none_when_no_valid_lines() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "A\nB\nC";
        
        let vendor: Option<String> = client.extract_vendor(text);
        
        assert_eq!(vendor, None);
    }

    // Extract Vendor should trim whitespace and still return the correct vendor name
    #[test]
    fn test_extract_vendor_with_leading_whitespace() {
        let client: TesseractClient = TesseractClient::new();
        let text: &str = "   \n   Trader Joe's   \n   Address Line";
        
        let vendor: Option<String> = client.extract_vendor(text);
        
        assert_eq!(vendor, Some("Trader Joe's".to_string()));
    }

    // Confidence score should be .15 when no fields are found and text is long
    #[tokio::test]
    async fn test_confidence_score_with_no_fields_and_long_text() {
        let client: TesseractClient = TesseractClient::new();
        let long_text: String = "A".repeat(100);
        
        let vendor: Option<String> = client.extract_vendor(&long_text);
        let amount: Option<f64> = client.extract_amount(&long_text);
        
        let mut score: f32 = 0.0;
        if vendor.is_some() { score += 0.4; }
        if amount.is_some() { score += 0.4; }
        if long_text.len() > 50 { score += 0.15; }
        
        assert_eq!(score, 0.15);
        assert_eq!(vendor, None);
        assert_eq!(amount, None);
    }

    // Confidence score should be .4 when only vendor is found
    #[tokio::test]
    async fn test_confidence_score_with_vendor_only() {
        let client: TesseractClient = TesseractClient::new();
        let mock_text: String = "Store Name".to_string();
        
        let vendor: Option<String> = client.extract_vendor(&mock_text);
        let amount: Option<f64> = client.extract_amount(&mock_text);
        
        let mut score: f32 = 0.0;
        if vendor.is_some() { score += 0.4; }
        if amount.is_some() { score += 0.4; }
        if mock_text.len() > 50 { score += 0.15; }
        
        assert_eq!(score, 0.4);
        assert!(vendor.is_some());
        assert_eq!(amount, None);
    }

    // Confidence score should be .4 when only amount is found
    #[tokio::test]
    async fn test_confidence_score_with_amount_only() {
        let client: TesseractClient = TesseractClient::new();
        let text_with_amount_only: String = "45.99".to_string();
        
        let vendor: Option<String> = client.extract_vendor(&text_with_amount_only);
        let amount: Option<f64> = client.extract_amount(&text_with_amount_only);
        
        let mut score: f32 = 0.0;
        if vendor.is_some() { score += 0.4; }
        if amount.is_some() { score += 0.4; }
        if text_with_amount_only.len() > 50 { score += 0.15; }
        
        assert_eq!(score, 0.4);
        assert_eq!(vendor, None);
        assert!(amount.is_some());
    }

    // Confidence score should be .8 when both fields are found but text is short
    #[tokio::test]
    async fn test_confidence_score_with_all_fields_and_short_text() {
        let client: TesseractClient = TesseractClient::new();
        let mock_text: String = "Costco\nTotal: 125.75".to_string();
        
        let vendor: Option<String> = client.extract_vendor(&mock_text);
        let amount: Option<f64> = client.extract_amount(&mock_text);
        
        let mut score: f32 = 0.0;
        if vendor.is_some() { score += 0.4; }
        if amount.is_some() { score += 0.4; }
        if mock_text.len() > 50 { score += 0.15; }
        
        assert_eq!(score, 0.8);
        assert!(vendor.is_some());
        assert!(amount.is_some());
    }

    // Confidence score should be .95 when both fields are found and text is long
    #[tokio::test]
    async fn test_confidence_score_with_all_fields_and_long_text() {
        let client: TesseractClient = TesseractClient::new();
        
        let long_text: String = format!(
            "Costco Wholesale\n123 Main Street\nCity, State 12345\n{}\nTotal: 125.75",
            "Item line\n".repeat(10)
        );
        
        let vendor: Option<String> = client.extract_vendor(&long_text);
        let amount: Option<f64> = client.extract_amount(&long_text);
        
        let mut score: f32 = 0.0;
        if vendor.is_some() { score += 0.4; }
        if amount.is_some() { score += 0.4; }
        if long_text.len() > 50 { score += 0.15; }
        
        assert!((score - 0.95).abs() < f32::EPSILON); // 0.95 cannot be represented exactly as a float, so check if it's close enough
        assert!(vendor.is_some());
        assert!(amount.is_some());
    }

    // Confidence score should be 0 when no fields are found and text is short
    #[tokio::test]
    async fn test_confidence_score_with_no_data() {
        let client: TesseractClient = TesseractClient::new();
        let empty_text: String = "".to_string();
        
        let vendor: Option<String> = client.extract_vendor(&empty_text);
        let amount: Option<f64> = client.extract_amount(&empty_text);
        
        let mut score: f32 = 0.0;
        if vendor.is_some() { score += 0.4; }
        if amount.is_some() { score += 0.4; }
        if empty_text.len() > 50 { score += 0.15; }
        
        assert_eq!(score, 0.0);
        assert_eq!(vendor, None);
        assert_eq!(amount, None);
    }
}
