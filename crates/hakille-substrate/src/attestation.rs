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
    Click {
        x: i32,
        y: i32,
        button: MouseButton,
    },
    DoubleClick {
        x: i32,
        y: i32,
    },
    RightClick {
        x: i32,
        y: i32,
    },
    Type {
        text: String,
    },
    Hotkey {
        keys: Vec<Key>,
    },
    Scroll {
        direction: ScrollDirection,
        amount: i32,
    },
    Drag {
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    },
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
    /// Measured die/package temperature in °C when a native thermal binding
    /// is available. `NaN` when unknown — see [`ThermalState::Unknown`].
    /// Use [`ThermalState::is_unknown`] instead of comparing this field
    /// directly (`NaN != NaN`).
    pub temperature_celsius: f32,
    pub throttled: bool,
}

impl ThermalState {
    /// Explicit unknown thermal state: no native thermal binding is linked
    /// for this substrate (no `/sys/class/thermal`, IOKit, WMI, thermal
    /// HAL, or Compute Pressure API wiring). This is the honest fallback —
    /// never a fabricated temperature.
    ///
    /// Named `Unknown` (not `UNKNOWN`) to read as the explicit unknown
    /// variant at call sites: `ThermalState::Unknown`.
    #[allow(non_upper_case_globals)]
    pub const Unknown: Self = Self {
        temperature_celsius: f32::NAN,
        throttled: false,
    };

    /// `true` when no thermal measurement is available
    /// ([`ThermalState::Unknown`]).
    pub fn is_unknown(&self) -> bool {
        self.temperature_celsius.is_nan()
    }

    /// `true` when this carries a real measurement.
    pub fn is_known(&self) -> bool {
        !self.is_unknown()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComputeBackend {
    Cpu,
    Cuda,
    Metal,
    Npu,
    WebGpu,
    Vulkan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraFacing {
    Front,
    Back,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub facing: CameraFacing,
}

impl CameraConfig {
    /// Conservative cross-platform probe: always `false`.
    ///
    /// Camera availability is proven by attempting `start_camera_capture` on
    /// the active substrate and honoring its result. The Windows substrate
    /// wires a real MediaFoundation `IMFSourceReader` backend; the other
    /// substrates still return an explicit
    /// `SubstrateError("camera not implemented …")` naming their missing
    /// binding (V4L2 / AVFoundation / orbital scheme / Camera2 /
    /// `getUserMedia`) instead of synthesizing fake frames. Never fabricate
    /// a `true` here.
    pub fn is_available(&self) -> bool {
        let _ = (self.width, self.height, self.fps);
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CameraStreamHandle(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub format: ImageFormat,
    pub timestamp_ms: u64,
}
