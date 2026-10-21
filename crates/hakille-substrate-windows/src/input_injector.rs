use crate::screen_capture::WindowsScreenCapturer;
use crate::win32_ffi::*;
use hakille_core::Result;
use hakille_substrate::{InputAction, Key, MouseButton, ScrollDirection};

pub struct WindowsInputInjector;

impl WindowsInputInjector {
    /// Inject a native GUI input action into the Windows session.
    ///
    /// HONNÊTETÉ : hors Windows, la fonction **échoue explicitement**. Les
    /// stubs Win32 ne font RIEN : renvoyer `Ok(())` serait un faux succès
    /// (« le clic a eu lieu » alors qu'aucun événement n'a été produit).
    pub fn inject(action: &InputAction) -> Result<()> {
        #[cfg(not(windows))]
        {
            let _ = action;
            return Err(hakille_core::HakilleError::SubstrateError(
                "injection d'entrée non implémentée hors Windows — refus explicite (aucun faux succès)"
                    .into(),
            ));
        }
        #[cfg(windows)]
        {
            let (screen_w, screen_h) = WindowsScreenCapturer::display_dimensions();

            match action {
                InputAction::Click { x, y, button } => {
                    let px = Self::resolve_coord(*x, screen_w);
                    let py = Self::resolve_coord(*y, screen_h);
                    // HONNÊTE : déplacement absolu SendInput 0..65535 (plus précis
                    // et layout-indépendant que SetCursorPos seul).
                    Self::move_absolute(
                        Self::to_absolute(px, screen_w),
                        Self::to_absolute(py, screen_h),
                    );

                    unsafe {
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

                    Self::move_absolute(
                        Self::to_absolute(px, screen_w),
                        Self::to_absolute(py, screen_h),
                    );
                    unsafe {
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

                    Self::move_absolute(
                        Self::to_absolute(px, screen_w),
                        Self::to_absolute(py, screen_h),
                    );
                    unsafe {
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

                    Self::move_absolute(
                        Self::to_absolute(start_x, screen_w),
                        Self::to_absolute(start_y, screen_h),
                    );
                    unsafe {
                        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                        for step in 1..=10 {
                            let cur_x = start_x + ((end_x - start_x) * step) / 10;
                            let cur_y = start_y + ((end_y - start_y) * step) / 10;
                            Self::move_absolute(
                                Self::to_absolute(cur_x, screen_w),
                                Self::to_absolute(cur_y, screen_h),
                            );
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
                    // HONNÊTE : injection Unicode via SendInput (layout-indépendant,
                    // fonctionne en AZERTY + ponctuation FR). Chaque caractère est
                    // envoyé en KEYEVENTF_UNICODE (down+up) ; pas de VK simulé.
                    for ch in text.chars() {
                        Self::type_unicode_char(ch);
                    }
                }

                InputAction::Hotkey { keys } => {
                    let vks: Vec<u8> = keys.iter().filter_map(Self::key_to_vk).collect();
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

    /// Convertit un pixel écran en coordonnée absolue SendInput 0..65535.
    /// HONNÊTE : `MOUSEEVENTF_ABSOLUTE` exige cette normalisation ; l'ancien
    /// code utilisait `SetCursorPos` seul (relatif implicite).
    pub fn to_absolute(px: i32, screen_max: i32) -> i32 {
        if screen_max <= 0 {
            return 0;
        }
        let clamped = px.clamp(0, screen_max);
        ((clamped as i64 * 65535) / screen_max as i64) as i32
    }

    /// Déplace le curseur en absolu via SendInput (0..65535), avec repli
    /// honnête sur SetCursorPos si SendInput échoue (session verrouillée…).
    fn move_absolute(abs_x: i32, abs_y: i32) {
        unsafe {
            let mut input = INPUT {
                type_: INPUT_MOUSE,
                ..Default::default()
            };
            input.u.mi.dx = abs_x;
            input.u.mi.dy = abs_y;
            input.u.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE;
            let sent = SendInput(
                1,
                &input as *const INPUT,
                std::mem::size_of::<INPUT>() as i32,
            );
            if sent == 0 {
                // Repli : position pixel classique.
                let (sw, sh) = WindowsScreenCapturer::display_dimensions();
                let px = ((abs_x as i64 * sw as i64) / 65535) as i32;
                let py = ((abs_y as i64 * sh as i64) / 65535) as i32;
                SetCursorPos(px, py);
            }
        }
    }

    /// Envoie un clic souris complet en absolu (move + down/up) via SendInput.
    #[allow(dead_code)]
    fn click_absolute(abs_x: i32, abs_y: i32, down: u32, up: u32) {
        Self::move_absolute(abs_x, abs_y);
        unsafe {
            for flag in [down, up] {
                let mut input = INPUT {
                    type_: INPUT_MOUSE,
                    ..Default::default()
                };
                input.u.mi.dwFlags = flag;
                SendInput(
                    1,
                    &input as *const INPUT,
                    std::mem::size_of::<INPUT>() as i32,
                );
            }
        }
    }

    /// Frappe un caractère Unicode via SendInput (down + up).
    /// Layout-indépendant : couvre AZERTY + ponctuation FR
    /// (é è à ç ù â ê î ô û ä ë ï ö ü ÿ œ æ …) sans table VK.
    fn type_unicode_char(ch: char) {
        // BMP uniquement pour KEYEVENTF_UNICODE (u16) ; hors-BMP → surrogate pair.
        let mut buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut buf);
        unsafe {
            for &unit in encoded.iter() {
                for up in [false, true] {
                    let mut input = INPUT {
                        type_: INPUT_KEYBOARD,
                        ..Default::default()
                    };
                    input.u.ki.wVk = 0;
                    input.u.ki.wScan = unit;
                    input.u.ki.dwFlags = KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 };
                    SendInput(
                        1,
                        &input as *const INPUT,
                        std::mem::size_of::<INPUT>() as i32,
                    );
                }
            }
        }
    }

    /// Indique si le caractère exige la touche Shift sur un layout AZERTY-FR.
    /// Utilisé par les chemins VK legacy (tests) ; le chemin de production
    /// est Unicode (aucun Shift requis).
    pub fn char_needs_shift(c: char) -> bool {
        if c.is_ascii_uppercase() {
            return true;
        }
        matches!(
            c,
            'A'..='Z'
                | 'É'
                | 'È'
                | 'À'
                | 'Ç'
                | 'Ù'
                | 'Â'
                | 'Ê'
                | 'Î'
                | 'Ô'
                | 'Û'
                | 'Ä'
                | 'Ë'
                | 'Ï'
                | 'Ö'
                | 'Ü'
                | '!'
                | '?'
                | '.'
                | '/'
                | '§'
                | '%'
                | 'µ'
                | '+'
        )
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

    /// Table VK de compatibilité (layout AZERTY-FR), couverte par les tests
    /// unitaires ; le chemin de frappe réel passe par `type_unicode_char`.
    #[allow(dead_code)]
    fn char_to_virtual_key(c: char) -> Option<u8> {
        // AZERTY-FR étendu : lettres, chiffres, ponctuation FR courante.
        // Le chemin de production privilégie l'Unicode (type_unicode_char) ;
        // cette table reste pour compatibilité VK + tests.
        let ascii = c.to_ascii_uppercase() as u8;
        if ascii.is_ascii_uppercase() || ascii.is_ascii_digit() {
            return Some(ascii);
        }
        match c {
            ' ' => Some(0x20),
            '\n' | '\r' => Some(0x0D),
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '(' | ')' | '-' | '_' | '@' | '#'
            | '$' | '%' | '&' | '*' | '+' | '=' | '/' | '\\' | '<' | '>' | '§' | 'µ' | '€' => {
                Some((c as u8).max(0x20))
            }
            // Lettres accentuées FR → VK de la lettre de base (Shift géré par
            // char_needs_shift) ; frappe réelle via Unicode de toute façon.
            'é' | 'É' | 'è' | 'È' | 'ê' | 'Ê' | 'ë' | 'Ë' => Some(b'E'),
            'à' | 'À' | 'â' | 'Â' | 'ä' | 'Ä' | 'æ' | 'Æ' => Some(b'A'),
            'ç' | 'Ç' => Some(b'C'),
            'ù' | 'Ù' | 'û' | 'Û' | 'ü' | 'Ü' => Some(b'U'),
            'î' | 'Î' | 'ï' | 'Ï' => Some(b'I'),
            'ô' | 'Ô' | 'ö' | 'Ö' | 'œ' | 'Œ' => Some(b'O'),
            'ÿ' | 'Ÿ' => Some(b'Y'),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WindowsInputInjector;

    #[test]
    fn azerty_shift_requirements() {
        for c in ['A', 'Z', 'É', 'È', 'À', 'Ç', '!', '?', '§'] {
            assert!(
                WindowsInputInjector::char_needs_shift(c),
                "{c} doit exiger Shift"
            );
        }
        for c in ['a', 'z', 'é', 'è', 'à', 'ç', '1', '-', '7'] {
            assert!(
                !WindowsInputInjector::char_needs_shift(c),
                "{c} ne doit pas exiger Shift"
            );
        }
    }

    #[test]
    fn virtual_key_table_covers_french_layout() {
        assert_eq!(WindowsInputInjector::char_to_virtual_key('a'), Some(b'A'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('7'), Some(b'7'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key(' '), Some(0x20));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('\n'), Some(0x0D));
        // Lettres accentuées : VK de la lettre de base (Shift via char_needs_shift).
        assert_eq!(WindowsInputInjector::char_to_virtual_key('é'), Some(b'E'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('È'), Some(b'E'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('à'), Some(b'A'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('ç'), Some(b'C'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('ù'), Some(b'U'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('î'), Some(b'I'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('ô'), Some(b'O'));
        assert_eq!(WindowsInputInjector::char_to_virtual_key('ÿ'), Some(b'Y'));
        // Hors table : pas de VK déduit (le chemin Unicode prend le relais).
        assert_eq!(WindowsInputInjector::char_to_virtual_key('中'), None);
        assert_eq!(WindowsInputInjector::char_to_virtual_key('✓'), None);
    }
}
