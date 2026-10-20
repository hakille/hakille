//! Clipboard — trait + impl mémoire (tests) ; natif derrière `native`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("clipboard indisponible: {0}")]
    Unavailable(String),
}

pub trait Clipboard: Send {
    fn copy_text(&mut self, text: &str) -> Result<(), ClipboardError>;
    fn paste_text(&self) -> Result<String, ClipboardError>;
}

#[derive(Default)]
pub struct InMemoryClipboard {
    content: String,
}

impl Clipboard for InMemoryClipboard {
    fn copy_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.content = text.to_string();
        Ok(())
    }
    fn paste_text(&self) -> Result<String, ClipboardError> {
        Ok(self.content.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_paste_roundtrip() {
        let mut c = InMemoryClipboard::default();
        c.copy_text("hello").unwrap();
        assert_eq!(c.paste_text().unwrap(), "hello");
    }
}
