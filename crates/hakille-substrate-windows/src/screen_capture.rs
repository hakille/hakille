use crate::win32_ffi::*;
use hakille_core::{HakilleError, Result};
use hakille_substrate::{ImageFormat, ScreenCapture};
use std::mem::size_of;

pub struct WindowsScreenCapturer;

impl WindowsScreenCapturer {
    /// Capture the primary Windows desktop screen using native Win32 GDI APIs.
    ///
    /// HONNÊTETÉ : si la capture native est impossible (session sans écran,
    /// session verrouillée, plate-forme non-Windows), la fonction **échoue
    /// explicitement**. Elle ne renvoie JAMAIS une image fabriquée : une frame
    /// synthétique présentée comme un écran serait un faux succès.
    pub fn capture_primary_display() -> Result<ScreenCapture> {
        #[cfg(not(windows))]
        {
            let _ = Self::display_dimensions();
            return Err(HakilleError::SubstrateError(
                "capture d'écran non implémentée hors Windows — refus explicite (aucune frame fabriquée)"
                    .into(),
            ));
        }
        #[cfg(windows)]
        {
            let (width, height) = Self::display_dimensions();

            unsafe {
                let hdc_screen = GetDC(0);
                if hdc_screen != 0 {
                    let hdc_mem = CreateCompatibleDC(hdc_screen);
                    if hdc_mem != 0 {
                        let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
                        if hbm != 0 {
                            let old_obj = SelectObject(hdc_mem, hbm);

                            // BitBlt from screen to memory DC
                            let blit_ok =
                                BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, 0, 0, SRCCOPY);

                            if blit_ok != 0 {
                                let mut bmi = BITMAPINFO::default();
                                bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
                                bmi.bmiHeader.biWidth = width;
                                // Negative height specifies top-down DIB
                                bmi.bmiHeader.biHeight = -height;
                                bmi.bmiHeader.biPlanes = 1;
                                bmi.bmiHeader.biBitCount = 32;
                                bmi.bmiHeader.biCompression = BI_RGB;

                                let pixel_count = (width * height) as usize;
                                let mut raw_pixels = vec![0u8; pixel_count * 4];

                                let dib_ok = GetDIBits(
                                    hdc_mem,
                                    hbm,
                                    0,
                                    height as u32,
                                    raw_pixels.as_mut_ptr(),
                                    &mut bmi,
                                    DIB_RGB_COLORS,
                                );

                                // Clean up GDI handles
                                SelectObject(hdc_mem, old_obj);
                                DeleteObject(hbm);
                                DeleteDC(hdc_mem);
                                ReleaseDC(0, hdc_screen);

                                if dib_ok != 0 {
                                    // GDI 32-bit is BGRA. Convert in-place to RGBA
                                    for chunk in raw_pixels.chunks_exact_mut(4) {
                                        let b = chunk[0];
                                        let r = chunk[2];
                                        chunk[0] = r;
                                        chunk[2] = b;
                                        // Ensure full opacity
                                        chunk[3] = 255;
                                    }

                                    return Ok(ScreenCapture {
                                        width: width as u32,
                                        height: height as u32,
                                        data: raw_pixels,
                                        format: ImageFormat::Rgba8,
                                    });
                                }
                            } else {
                                SelectObject(hdc_mem, old_obj);
                                DeleteObject(hbm);
                                DeleteDC(hdc_mem);
                                ReleaseDC(0, hdc_screen);
                            }
                        } else {
                            DeleteDC(hdc_mem);
                            ReleaseDC(0, hdc_screen);
                        }
                    } else {
                        ReleaseDC(0, hdc_screen);
                    }
                }
            }

            // La capture native a échoué (session sans écran, session verrouillée,
            // GDI indisponible) : on REFUSE explicitement. Aucune frame
            // fabriquée ne doit être présentée comme un écran réel.
            Err(HakilleError::SubstrateError(format!(
                "capture d'écran impossible (session sans écran ou GDI indisponible, {width}x{height}) — aucune frame fabriquée"
            )))
        }
    }

    /// Query primary monitor width and height in pixels.
    pub fn display_dimensions() -> (i32, i32) {
        unsafe {
            let cx = GetSystemMetrics(SM_CXSCREEN);
            let cy = GetSystemMetrics(SM_CYSCREEN);
            if cx > 0 && cy > 0 {
                (cx, cy)
            } else {
                (1920, 1080)
            }
        }
    }
}
