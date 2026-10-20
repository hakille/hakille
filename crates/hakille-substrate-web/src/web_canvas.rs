use hakille_substrate::{ImageFormat, ScreenCapture};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebCanvasConfig {
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub device_pixel_ratio: f64,
    pub capture_cursor: bool,
}

impl Default for WebCanvasConfig {
    fn default() -> Self {
        Self {
            viewport_width: 1920,
            viewport_height: 1080,
            device_pixel_ratio: 1.0,
            capture_cursor: true,
        }
    }
}

pub struct WebScreenCaptureAdapter {
    config: WebCanvasConfig,
}

impl WebScreenCaptureAdapter {
    pub fn new(config: WebCanvasConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &WebCanvasConfig {
        &self.config
    }

    pub fn ingest_image_data(
        &self,
        width: u32,
        height: u32,
        rgba_bytes: &[u8],
    ) -> Result<ScreenCapture, String> {
        if width == 0 || height == 0 {
            return Err("Invalid dimensions: width and height must be non-zero".to_string());
        }
        let expected_len = (width as usize) * (height as usize) * 4;
        if rgba_bytes.len() != expected_len {
            return Err(format!(
                "Invalid RGBA buffer length: expected {} bytes, got {}",
                expected_len,
                rgba_bytes.len()
            ));
        }

        Ok(ScreenCapture {
            width,
            height,
            data: rgba_bytes.to_vec(),
            format: ImageFormat::Rgba8,
        })
    }

    pub fn capture_frame(&self) -> ScreenCapture {
        let width = self.config.viewport_width;
        let height = self.config.viewport_height;
        let pixel_count = (width * height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        // Render browser tab, address bar, and web canvas body
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                if y < 40 {
                    // Browser tab bar (dark charcoal)
                    data[idx] = 32;
                    data[idx + 1] = 33;
                    data[idx + 2] = 36;
                    data[idx + 3] = 255;
                } else if y < 80 {
                    // Browser URL omnibox
                    data[idx] = 48;
                    data[idx + 1] = 49;
                    data[idx + 2] = 54;
                    data[idx + 3] = 255;
                } else {
                    // Web page canvas viewport
                    let r = ((x * 40) / width) as u8 + 20;
                    let g = ((y * 40) / height) as u8 + 25;
                    let b = 45u8;
                    data[idx] = r;
                    data[idx + 1] = g;
                    data[idx + 2] = b;
                    data[idx + 3] = 255;
                }
            }
        }

        ScreenCapture {
            width,
            height,
            data,
            format: ImageFormat::Rgba8,
        }
    }
}

impl Default for WebScreenCaptureAdapter {
    fn default() -> Self {
        Self::new(WebCanvasConfig::default())
    }
}
