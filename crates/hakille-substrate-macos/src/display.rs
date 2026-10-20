use hakille_substrate::{ImageFormat, ScreenCapture};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CGRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacosDisplayDescriptor {
    pub display_id: u32,
    pub bounds: CGRect,
    pub scale_factor: f64,
    pub is_main: bool,
}

pub struct MacosQuartzDisplayCapture {
    main_display: MacosDisplayDescriptor,
}

impl MacosQuartzDisplayCapture {
    /// Constructs the SIMULATED main-display descriptor used off
    /// CoreGraphics: a fixed 2560x1600 logical @2x profile, **not** a real
    /// `CGDisplay` query. See [`MacosQuartzDisplayCapture::capture_screen`].
    pub fn new() -> Self {
        Self {
            main_display: MacosDisplayDescriptor {
                display_id: 1,
                bounds: CGRect {
                    x: 0.0,
                    y: 0.0,
                    width: 2560.0,
                    height: 1600.0,
                },
                scale_factor: 2.0, // Retina 2x scale
                is_main: true,
            },
        }
    }

    pub fn with_descriptor(main_display: MacosDisplayDescriptor) -> Self {
        Self { main_display }
    }

    pub fn display_descriptor(&self) -> &MacosDisplayDescriptor {
        &self.main_display
    }

    pub fn physical_width(&self) -> u32 {
        (self.main_display.bounds.width * self.main_display.scale_factor) as u32
    }

    pub fn physical_height(&self) -> u32 {
        (self.main_display.bounds.height * self.main_display.scale_factor) as u32
    }

    /// Synthetic desktop buffer for structural tests and demos.
    ///
    /// SIMULATED: no CoreGraphics / CGDisplayStream binding is linked, so
    /// this does **not** read the real screen — the pixels below are a
    /// generated placeholder and must never be presented as a capture of
    /// the host display. The real capture path requires a CoreGraphics
    /// screen-recording binding (out of scope here).
    pub fn capture_screen(&self) -> ScreenCapture {
        let width = self.main_display.bounds.width as u32;
        let height = self.main_display.bounds.height as u32;
        let pixel_count = (width * height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        // Render macOS Tahoe / Sonoma style desktop buffer
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                if y < 44 {
                    // macOS Menu bar (translucent glass gray)
                    data[idx] = 45; // R
                    data[idx + 1] = 45; // G
                    data[idx + 2] = 50; // B
                    data[idx + 3] = 255; // A
                } else if y > height - 80 && x > (width / 4) && x < (width * 3 / 4) {
                    // macOS Floating Dock shelf
                    data[idx] = 60;
                    data[idx + 1] = 60;
                    data[idx + 2] = 70;
                    data[idx + 3] = 230;
                } else {
                    // Desktop wallpaper gradient
                    let r = ((x * 40) / width) as u8 + 15;
                    let g = ((y * 30) / height) as u8 + 20;
                    let b = 55u8;
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

impl Default for MacosQuartzDisplayCapture {
    fn default() -> Self {
        Self::new()
    }
}
