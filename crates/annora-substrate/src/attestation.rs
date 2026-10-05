use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub trait AttestationProvider: Send + Sync + 'static {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub format: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AudioStreamHandle(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageFormat {
    Rgb8,
    Rgba8,
    Bgra8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCapture {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Key {
    Enter,
    Escape,
    Space,
    Backspace,
    Tab,
    Shift,
    Ctrl,
    Alt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputAction {
    Click { x: i32, y: i32, button: MouseButton },
    DoubleClick { x: i32, y: i32 },
    RightClick { x: i32, y: i32 },
    Type { text: String },
    Hotkey { keys: Vec<Key> },
    Scroll { direction: ScrollDirection, amount: i32 },
    Drag { from_x: i32, from_y: i32, to_x: i32, to_y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessHandle(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerState {
    pub on_battery: bool,
    pub battery_percent: f32,
    pub charging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalState {
    pub temperature_celsius: f32,
    pub throttled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputeBackend {
    Cpu,
    Cuda,
    Metal,
    Npu,
    WebGpu,
    Vulkan,
}
