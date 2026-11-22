use hakille_substrate::{InputAction, Key, MouseButton, ScrollDirection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;

pub const SYN_REPORT: u16 = 0;

pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_WHEEL: u16 = 0x08;

pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;

pub const KEY_ESC: u16 = 1;
pub const KEY_1: u16 = 2;
pub const KEY_0: u16 = 11;
pub const KEY_MINUS: u16 = 12;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_TAB: u16 = 15;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_A: u16 = 30;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_COMMA: u16 = 51;
pub const KEY_DOT: u16 = 52;
pub const KEY_SLASH: u16 = 53;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_SPACE: u16 = 57;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct LinuxInputEvent {
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

impl LinuxInputEvent {
    pub fn new(type_: u16, code: u16, value: i32) -> Self {
        Self { type_, code, value }
    }

    pub fn syn() -> Self {
        Self {
            type_: EV_SYN,
            code: SYN_REPORT,
            value: 0,
        }
    }

    pub fn key(code: u16, pressed: bool) -> Self {
        Self {
            type_: EV_KEY,
            code,
            value: if pressed { 1 } else { 0 },
        }
    }

    pub fn rel(code: u16, value: i32) -> Self {
        Self {
            type_: EV_REL,
            code,
            value,
        }
    }

    pub fn abs(code: u16, value: i32) -> Self {
        Self {
            type_: EV_ABS,
            code,
            value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UinputDeviceConfig {
    pub device_name: String,
    pub supports_mouse: bool,
    pub supports_keyboard: bool,
}

impl Default for UinputDeviceConfig {
    fn default() -> Self {
        Self {
            device_name: "Hakille Virtual Uinput Device".to_string(),
            supports_mouse: true,
            supports_keyboard: true,
        }
    }
}

pub struct LinuxInputInjector {
    config: UinputDeviceConfig,
    recorded_events: Mutex<Vec<LinuxInputEvent>>,
}

impl LinuxInputInjector {
    pub fn new(config: UinputDeviceConfig) -> Self {
        Self {
            config,
            recorded_events: Mutex::new(Vec::new()),
        }
    }

    pub fn config(&self) -> &UinputDeviceConfig {
        &self.config
    }

    pub fn recorded_events(&self) -> Vec<LinuxInputEvent> {
        self.recorded_events
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn clear_recorded_events(&self) {
        if let Ok(mut g) = self.recorded_events.lock() {
            g.clear();
        }
    }

    /// Table exacte des codes evdev Linux (`input-event-codes.h`, disposition QWERTY).
    pub fn ascii_letter_to_evdev(ch: char) -> Option<u16> {
        match ch.to_ascii_lowercase() {
            'q' => Some(16),
            'w' => Some(17),
            'e' => Some(18),
            'r' => Some(19),
            't' => Some(20),
            'y' => Some(21),
            'u' => Some(22),
            'i' => Some(23),
            'o' => Some(24),
            'p' => Some(25),
            'a' => Some(30),
            's' => Some(31),
            'd' => Some(32),
            'f' => Some(33),
            'g' => Some(34),
            'h' => Some(35),
            'j' => Some(36),
            'k' => Some(37),
            'l' => Some(38),
            'z' => Some(44),
            'x' => Some(45),
            'c' => Some(46),
            'v' => Some(47),
            'b' => Some(48),
            'n' => Some(49),
            'm' => Some(50),
            _ => None,
        }
    }

    pub fn key_to_evdev(key: &Key) -> u16 {
        match key {
            Key::Enter => KEY_ENTER,
            Key::Escape => KEY_ESC,
            Key::Space => KEY_SPACE,
            Key::Backspace => KEY_BACKSPACE,
            Key::Tab => KEY_TAB,
            Key::Shift => KEY_LEFTSHIFT,
            Key::Ctrl => KEY_LEFTCTRL,
            Key::Alt => KEY_LEFTALT,
        }
    }

    pub fn button_to_evdev(btn: &MouseButton) -> u16 {
        match btn {
            MouseButton::Left => BTN_LEFT,
            MouseButton::Right => BTN_RIGHT,
            MouseButton::Middle => BTN_MIDDLE,
        }
    }

    pub fn translate_action(&self, action: &InputAction) -> Vec<LinuxInputEvent> {
        let mut events = Vec::new();

        match action {
            InputAction::Click { x, y, button } => {
                events.push(LinuxInputEvent::abs(ABS_X, *x));
                events.push(LinuxInputEvent::abs(ABS_Y, *y));
                events.push(LinuxInputEvent::rel(REL_X, *x));
                events.push(LinuxInputEvent::rel(REL_Y, *y));
                events.push(LinuxInputEvent::syn());

                let code = Self::button_to_evdev(button);
                events.push(LinuxInputEvent::key(code, true));
                events.push(LinuxInputEvent::syn());
                events.push(LinuxInputEvent::key(code, false));
                events.push(LinuxInputEvent::syn());
            }
            InputAction::DoubleClick { x, y } => {
                events.push(LinuxInputEvent::abs(ABS_X, *x));
                events.push(LinuxInputEvent::abs(ABS_Y, *y));
                events.push(LinuxInputEvent::rel(REL_X, *x));
                events.push(LinuxInputEvent::rel(REL_Y, *y));
                events.push(LinuxInputEvent::syn());

                for _ in 0..2 {
                    events.push(LinuxInputEvent::key(BTN_LEFT, true));
                    events.push(LinuxInputEvent::syn());
                    events.push(LinuxInputEvent::key(BTN_LEFT, false));
                    events.push(LinuxInputEvent::syn());
                }
            }
            InputAction::RightClick { x, y } => {
                events.push(LinuxInputEvent::abs(ABS_X, *x));
                events.push(LinuxInputEvent::abs(ABS_Y, *y));
                events.push(LinuxInputEvent::rel(REL_X, *x));
                events.push(LinuxInputEvent::rel(REL_Y, *y));
                events.push(LinuxInputEvent::syn());

                events.push(LinuxInputEvent::key(BTN_RIGHT, true));
                events.push(LinuxInputEvent::syn());
                events.push(LinuxInputEvent::key(BTN_RIGHT, false));
                events.push(LinuxInputEvent::syn());
            }
            InputAction::Type { text } => {
                for ch in text.chars() {
                    if ch.is_ascii_uppercase() {
                        if let Some(code) = Self::ascii_letter_to_evdev(ch) {
                            events.push(LinuxInputEvent::key(KEY_LEFTSHIFT, true));
                            events.push(LinuxInputEvent::key(code, true));
                            events.push(LinuxInputEvent::syn());
                            events.push(LinuxInputEvent::key(code, false));
                            events.push(LinuxInputEvent::key(KEY_LEFTSHIFT, false));
                            events.push(LinuxInputEvent::syn());
                        }
                    } else {
                        let code = match ch {
                            ' ' => KEY_SPACE,
                            '\n' | '\r' => KEY_ENTER,
                            '\t' => KEY_TAB,
                            'a'..='z' => Self::ascii_letter_to_evdev(ch).unwrap_or(KEY_SPACE),
                            '1'..='9' => KEY_1 + (ch as u16 - '1' as u16),
                            '0' => KEY_0,
                            '.' => KEY_DOT,
                            ',' => KEY_COMMA,
                            '-' => KEY_MINUS,
                            '/' => KEY_SLASH,
                            _ => KEY_SPACE,
                        };
                        events.push(LinuxInputEvent::key(code, true));
                        events.push(LinuxInputEvent::syn());
                        events.push(LinuxInputEvent::key(code, false));
                        events.push(LinuxInputEvent::syn());
                    }
                }
            }
            InputAction::Hotkey { keys } => {
                for key in keys {
                    events.push(LinuxInputEvent::key(Self::key_to_evdev(key), true));
                }
                events.push(LinuxInputEvent::syn());
                for key in keys.iter().rev() {
                    events.push(LinuxInputEvent::key(Self::key_to_evdev(key), false));
                }
                events.push(LinuxInputEvent::syn());
            }
            InputAction::Scroll { direction, amount } => {
                let val = match direction {
                    ScrollDirection::Up => *amount,
                    ScrollDirection::Down => amount.saturating_neg(),
                    ScrollDirection::Left | ScrollDirection::Right => *amount,
                };
                events.push(LinuxInputEvent::rel(REL_WHEEL, val));
                events.push(LinuxInputEvent::syn());
            }
            InputAction::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => {
                events.push(LinuxInputEvent::rel(REL_X, *from_x));
                events.push(LinuxInputEvent::rel(REL_Y, *from_y));
                events.push(LinuxInputEvent::syn());
                events.push(LinuxInputEvent::key(BTN_LEFT, true));
                events.push(LinuxInputEvent::syn());
                events.push(LinuxInputEvent::rel(REL_X, *to_x));
                events.push(LinuxInputEvent::rel(REL_Y, *to_y));
                events.push(LinuxInputEvent::syn());
                events.push(LinuxInputEvent::key(BTN_LEFT, false));
                events.push(LinuxInputEvent::syn());
            }
        }

        events
    }

    pub fn inject(&self, _action: &InputAction) -> Result<(), String> {
        // HONNÊTE : tant que les ioctls UI_DEV_SETUP / UI_DEV_CREATE et l'écriture
        // noyau `struct input_event` ne sont pas raccordés, on ne retourne JAMAIS
        // un faux `Ok(())` même si `/dev/uinput` est ouvrable.
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/uinput")
        {
            Ok(_) => Err(
                "uinput non raccordé : /dev/uinput est accessible mais la création du périphérique virtuel (UI_DEV_CREATE) et l'émission d'événements noyau ne sont pas encore implémentées ; aucun événement émis".to_string(),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
                "uinput indisponible (/dev/uinput absent) : injection impossible — {}",
                e
            )),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(format!(
                "uinput EACCES (/dev/uinput permission refusée, groupe `input`/udev requis) : injection impossible — {}",
                e
            )),
            Err(e) => Err(format!(
                "uinput inaccessible (/dev/uinput) : injection impossible — {}",
                e
            )),
        }
    }
}

impl Default for LinuxInputInjector {
    fn default() -> Self {
        Self::new(UinputDeviceConfig::default())
    }
}
