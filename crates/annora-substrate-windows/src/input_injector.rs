use crate::screen_capture::WindowsScreenCapturer;
use crate::win32_ffi::*;
use annora_core::Result;
use annora_substrate::{InputAction, Key, MouseButton, ScrollDirection};

pub struct WindowsInputInjector;

impl WindowsInputInjector {
    /// Inject a native GUI input action into the Windows session.
    pub fn inject(action: &InputAction) -> Result<()> {
        let (screen_w, screen_h) = WindowsScreenCapturer::display_dimensions();

        match action {
            InputAction::Click { x, y, button } => {
                let px = Self::resolve_coord(*x, screen_w);
                let py = Self::resolve_coord(*y, screen_h);

                unsafe {
                    SetCursorPos(px, py);
                    match button {
                        MouseButton::Left => {
                            mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                            mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                        }
                        MouseButton::Right => {
                            mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
                            mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
                        }
                        MouseButton::Middle => {
                            mouse_event(MOUSEEVENTF_MIDDLEDOWN, 0, 0, 0, 0);
                            mouse_event(MOUSEEVENTF_MIDDLEUP, 0, 0, 0, 0);
                        }
                    }
                }
            }

            InputAction::DoubleClick { x, y } => {
                let px = Self::resolve_coord(*x, screen_w);
                let py = Self::resolve_coord(*y, screen_h);

                unsafe {
                    SetCursorPos(px, py);
                    mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                    mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                    mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                }
            }

            InputAction::RightClick { x, y } => {
                let px = Self::resolve_coord(*x, screen_w);
                let py = Self::resolve_coord(*y, screen_h);

                unsafe {
                    SetCursorPos(px, py);
                    mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
                    mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
                }
            }

            InputAction::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => {
                let start_x = Self::resolve_coord(*from_x, screen_w);
                let start_y = Self::resolve_coord(*from_y, screen_h);
                let end_x = Self::resolve_coord(*to_x, screen_w);
                let end_y = Self::resolve_coord(*to_y, screen_h);

                unsafe {
                    SetCursorPos(start_x, start_y);
                    mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                    for step in 1..=10 {
                        let cur_x = start_x + ((end_x - start_x) * step) / 10;
                        let cur_y = start_y + ((end_y - start_y) * step) / 10;
                        SetCursorPos(cur_x, cur_y);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                }
            }

            InputAction::Scroll { direction, amount } => {
                let ticks = (*amount).max(1) * 120;
                unsafe {
                    match direction {
                        ScrollDirection::Up => {
                            mouse_event(MOUSEEVENTF_WHEEL, 0, 0, ticks as u32, 0);
                        }
                        ScrollDirection::Down => {
                            mouse_event(MOUSEEVENTF_WHEEL, 0, 0, (-ticks) as u32, 0);
                        }
                        ScrollDirection::Right => {
                            mouse_event(MOUSEEVENTF_HWHEEL, 0, 0, ticks as u32, 0);
                        }
                        ScrollDirection::Left => {
                            mouse_event(MOUSEEVENTF_HWHEEL, 0, 0, (-ticks) as u32, 0);
                        }
                    }
                }
            }

            InputAction::Type { text } => {
                for ch in text.chars() {
                    let vk = Self::char_to_virtual_key(ch);
                    if let Some(code) = vk {
                        unsafe {
                            keybd_event(code, 0, 0, 0);
                            keybd_event(code, 0, KEYEVENTF_KEYUP, 0);
                        }
                    }
                }
            }

            InputAction::Hotkey { keys } => {
                let vks: Vec<u8> = keys.iter().filter_map(|k| Self::key_to_vk(k)).collect();
                unsafe {
                    for &vk in &vks {
                        keybd_event(vk, 0, 0, 0);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    for &vk in vks.iter().rev() {
                        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve coordinate: if coordinate is small (0 or 1), treat as normalized ratio, otherwise as pixel.
    fn resolve_coord(val: i32, screen_max: i32) -> i32 {
        if val < 0 {
            0
        } else if val > screen_max {
            screen_max
        } else {
            val
        }
    }

    fn key_to_vk(key: &Key) -> Option<u8> {
        match key {
            Key::Enter => Some(0x0D),
            Key::Escape => Some(0x1B),
            Key::Space => Some(0x20),
            Key::Backspace => Some(0x08),
            Key::Tab => Some(0x09),
            Key::Shift => Some(0x10),
            Key::Ctrl => Some(0x11),
            Key::Alt => Some(0x12),
        }
    }

    fn char_to_virtual_key(c: char) -> Option<u8> {
        let ascii = c.to_ascii_uppercase() as u8;
        if (b'A'..=b'Z').contains(&ascii) || (b'0'..=b'9').contains(&ascii) {
            Some(ascii)
        } else if c == ' ' {
            Some(0x20)
        } else if c == '\n' || c == '\r' {
            Some(0x0D)
        } else {
            None
        }
    }
}
