//! i18n — Fluent + ICU4X hors renderer (ici : tables minimales déterministes).
//!
//! FR/EN/AR-RTL/JA couverts dès la v1. Aucune chaîne UI codée en dur dans
//! les composants : tout passe par `Strings::get`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Locale {
    Fr,
    En,
    Ar,
    Ja,
}

impl Locale {
    pub fn is_rtl(&self) -> bool {
        matches!(self, Self::Ar)
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::Fr => "fr",
            Self::En => "en",
            Self::Ar => "ar",
            Self::Ja => "ja",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Strings {
    pub locale: Locale,
}

impl Strings {
    pub fn new(locale: Locale) -> Self {
        Self { locale }
    }

    pub fn get(&self, key: &str) -> String {
        let (fr, en, ar, ja) = match key {
            "hud.prompt" => (
                "Commande Hakille — Entrée pour envoyer, Échap pour fermer",
                "Hakille command — Enter to send, Esc to close",
                "أمر أكورا — إدخال للإرسال، هروب للإغلاق",
                "Hakilleコマンド — Enterで送信、Escで閉じる",
            ),
            "hud.validate" => ("Valider", "Confirm", "تأكيد", "確定"),
            "hud.modify" => ("Modifier", "Edit", "تعديل", "修正"),
            "hud.refuse" => ("Refuser", "Decline", "رفض", "拒否"),
            "journal.title" => (
                "Journal d'audit — frise, annulation en un geste",
                "Audit journal — timeline, one-gesture undo",
                "سجل التدقيق — مخطط زمني، تراجع بإيماءة",
                "監査ジャーナル — タイムライン、ワンジェスチャで取消",
            ),
            "presence.idle" => (
                "Hakille : veille ambiante",
                "Hakille: ambient idle",
                "أكورا: خمول محيطي",
                "Hakille: 待機中",
            ),
            "presence.listening" => (
                "Hakille : écoute active",
                "Hakille: listening",
                "أكورا: استماع نشط",
                "Hakille: リスニング中",
            ),
            "presence.decision" => (
                "Hakille : décision requise — activer pour voir",
                "Hakille: decision required — activate to view",
                "أكورا: مطلوب قرار — فعّل للعرض",
                "Hakille: 決定が必要です — 開いて確認",
            ),
            "card.hold" => (
                "Valider — appui long 600 millisecondes (action définitive)",
                "Confirm — 600 ms long-press (final action)",
                "تأكيد — ضغط مطوّل 600 مللي ثانية (إجراء نهائي)",
                "確定 — 600ミリ秒長押し（最終アクション）",
            ),
            _ => ("〈manquant〉", "〈missing〉", "〈مفقود〉", "〈不足〉"),
        };
        match self.locale {
            Locale::Fr => fr.into(),
            Locale::En => en.into(),
            Locale::Ar => ar.into(),
            Locale::Ja => ja.into(),
        }
    }

    /// Pluriel ICU simplifié : 0/1/pluriel + arabe (0/1/2/pluriel).
    pub fn plural(&self, key_one: &str, key_other: &str, n: u64) -> String {
        match self.locale {
            Locale::Ar if n == 0 => format!("لا {key_other}"),
            Locale::Ar if n == 2 => format!("{key_other}ان"),
            _ if n == 1 => key_one.to_string(),
            _ => format!("{n} {key_other}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_locales_cover_hud() {
        for l in [Locale::Fr, Locale::En, Locale::Ar, Locale::Ja] {
            let s = Strings::new(l);
            assert!(!s.get("hud.prompt").is_empty());
            assert!(!s.get("hud.validate").is_empty());
        }
        assert!(
            Strings::new(Locale::Ar)
                .get("hud.prompt")
                .chars()
                .any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
        );
    }
    #[test]
    fn rtl_only_arabic() {
        assert!(Locale::Ar.is_rtl());
        assert!(!Locale::Fr.is_rtl());
        assert!(!Locale::Ja.is_rtl());
    }
}
