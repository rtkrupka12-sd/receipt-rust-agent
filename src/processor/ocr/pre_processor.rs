use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader};
use std::io::Cursor;
use crate::error::ProcessorError;

pub struct ImagePreProcessor;

impl ImagePreProcessor {
    // Takes raw image bytes, optimizes the image for OCR layout analysis,
    // and returns the cleaned image bytes as a JPEG/PNG payload.
    pub fn optimize_for_ocr(raw_bytes: &[u8]) -> Result<Vec<u8>, ProcessorError> {
        // 1. Load the image from raw bytes memory
        let img: DynamicImage = ImageReader::new(Cursor::new(raw_bytes))
            .with_guessed_format()
            .map_err(|e| ProcessorError::StorageError(format!("Failed to read image format: {}", e)))?
            .decode()
            .map_err(|e| ProcessorError::StorageError(format!("Failed to decode image bytes: {}", e)))?;

        // 2. Downscale if the image is an overkill resolution (saves local Tesseract CPU cycles)
        let (width, height) = img.dimensions();
        let max_dimension = 2000;
        let mut processed_img: DynamicImage = if width > max_dimension || height > max_dimension {
            img.resize(max_dimension, max_dimension, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        // 3. Convert to Grayscale (removes color noise, shadows, and receipt background tints)
        processed_img = processed_img.thumbnail(processed_img.width(), processed_img.height()); // forces optimization
        let mut grayscale = processed_img.into_luma8();

        // 4. Enhance Contrast / Adaptive Thresholding approximation
        // Stretches the contrast manually to force text to go black and backgrounds to white
        image::imageops::contrast(&mut grayscale, 20.0); // Bumps contrast significantly

        // 5. Save the optimized image back to bytes in a Tesseract-friendly format (JPEG or PNG)
        let mut optimized_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut optimized_bytes);
        
        DynamicImage::ImageLuma8(grayscale)
            .write_to(&mut cursor, ImageFormat::Jpeg)
            .map_err(|e| ProcessorError::StorageError(format!("Failed to write optimized image buffer: {}", e)))?;

        Ok(optimized_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Invalid image bytes should return an error during pre-processing
    #[test]
    fn test_invalid_image_bytes_fail_gracefully() {
        let bad_bytes = vec![0, 1, 2, 3, 4, 5];
        let result = ImagePreProcessor::optimize_for_ocr(&bad_bytes);
        assert!(result.is_err());
    }
}