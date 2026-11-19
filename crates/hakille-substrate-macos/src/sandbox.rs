use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatbeltRule {
    pub action: String,            // "allow" or "deny"
    pub filter: String,            // e.g. "file-read*", "network-outbound", "mach-lookup"
    pub predicate: Option<String>, // e.g. "(subpath \"/Users/Shared/Hakille\")"
}

impl SeatbeltRule {
    pub fn allow(filter: &str, predicate: Option<&str>) -> Self {
        Self {
            action: "allow".to_string(),
            filter: filter.to_string(),
            predicate: predicate.map(|s| s.to_string()),
        }
    }

    pub fn deny(filter: &str, predicate: Option<&str>) -> Self {
        Self {
            action: "deny".to_string(),
            filter: filter.to_string(),
            predicate: predicate.map(|s| s.to_string()),
        }
    }

    pub fn to_seatbelt_dsl(&self) -> String {
        if let Some(pred) = &self.predicate {
            format!("({} {} {})", self.action, self.filter, pred)
        } else {
            format!("({} {})", self.action, self.filter)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacosSandboxProfile {
    pub profile_string: String,
    pub allow_file_read: bool,
    pub allow_file_write: bool,
    pub allow_network_outbound: bool,
    pub rules: Vec<SeatbeltRule>,
    pub parameters: HashMap<String, String>,
}

impl Default for MacosSandboxProfile {
    fn default() -> Self {
        Self::agent_minimal()
    }
}

/// Échappe une chaîne littérale Scheme/Seatbelt pour empêcher toute injection
/// de directives (guillemets, antislash, retours chariot, NUL).
fn escape_seatbelt_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => {}
            _ => out.push(ch),
        }
    }
    out
}

fn is_valid_seatbelt_ident(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'*'))
}

impl MacosSandboxProfile {
    pub fn new(rules: Vec<SeatbeltRule>) -> Self {
        let mut profile = Self {
            profile_string: String::new(),
            allow_file_read: false,
            allow_file_write: false,
            allow_network_outbound: false,
            rules,
            parameters: HashMap::new(),
        };
        profile.compile();
        profile
    }

    pub fn set_param(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.parameters.insert(key.into(), val.into());
        self.compile();
    }

    pub fn compile(&mut self) {
        let mut dsl = String::from("(version 1)\n(deny default)\n");

        self.allow_file_read = false;
        self.allow_file_write = false;
        self.allow_network_outbound = false;

        let mut sorted_params: Vec<(&String, &String)> = self.parameters.iter().collect();
        sorted_params.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_params {
            if is_valid_seatbelt_ident(k) {
                dsl.push_str(&format!(
                    "(define {} \"{}\")\n",
                    k,
                    escape_seatbelt_string(v)
                ));
            }
        }

        // Standard Darwin security minimums: POSIX shm, sysctl-read, /dev/urandom
        dsl.push_str("(allow sysctl-read)\n");
        dsl.push_str(
            "(allow file-read-data (literal \"/dev/random\") (literal \"/dev/urandom\"))\n",
        );

        for rule in &self.rules {
            dsl.push_str(&rule.to_seatbelt_dsl());
            dsl.push('\n');

            if rule.filter.contains("file-read") {
                self.allow_file_read = rule.action == "allow";
            }
            if rule.filter.contains("file-write") {
                self.allow_file_write = rule.action == "allow";
            }
            if rule.filter.contains("network") {
                self.allow_network_outbound = rule.action == "allow";
            }
        }

        self.profile_string = dsl;
    }

    /// Tente d'appliquer ce profil via l'API Darwin `sandbox_init(3)`.
    ///
    /// Honnêteté : aucun binding FFI `libsystem_sandbox.dylib` (`sandbox_init`)
    /// n'est lié dans ce binaire ; renvoie donc systématiquement une erreur
    /// explicite après validation syntaxique, jamais un faux `Ok(())`.
    pub fn apply(&self) -> Result<(), String> {
        self.validate_safety()?;
        Err(
            "Seatbelt non appliqué : binding libsystem_sandbox (sandbox_init) non raccordé ; profil compilé et validé uniquement"
                .to_string(),
        )
    }

    pub fn agent_minimal() -> Self {
        let rules = vec![
            SeatbeltRule::allow("file-read*", Some("(subpath \"/Users/Shared/Hakille\")")),
            SeatbeltRule::deny("network*", None),
            SeatbeltRule::deny("process-exec*", None),
        ];
        Self::new(rules)
    }

    pub fn agent_mail(home_dir: &str) -> Self {
        let safe_home = escape_seatbelt_string(home_dir);
        let rules = vec![
            SeatbeltRule::allow(
                "file-read*",
                Some(&format!("(subpath \"{safe_home}/Library/Hakille/Mail\")")),
            ),
            SeatbeltRule::allow(
                "file-write*",
                Some(&format!("(subpath \"{safe_home}/Library/Hakille/Mail\")")),
            ),
            SeatbeltRule::allow("network-outbound", Some("(remote tcp)")),
            SeatbeltRule::allow(
                "mach-lookup",
                Some("(global-name \"com.apple.MailServiceAgent\")"),
            ),
        ];
        Self::new(rules)
    }

    pub fn agent_calendar(home_dir: &str) -> Self {
        let safe_home = escape_seatbelt_string(home_dir);
        let rules = vec![
            SeatbeltRule::allow(
                "file-read*",
                Some(&format!(
                    "(subpath \"{safe_home}/Library/Hakille/Calendar\")"
                )),
            ),
            SeatbeltRule::allow(
                "file-write*",
                Some(&format!(
                    "(subpath \"{safe_home}/Library/Hakille/Calendar\")"
                )),
            ),
            SeatbeltRule::allow(
                "mach-lookup",
                Some("(global-name \"com.apple.CalendarAgent\")"),
            ),
            SeatbeltRule::deny("network-outbound", None),
        ];
        Self::new(rules)
    }

    pub fn agent_code_sandbox(sandbox_dir: &str) -> Self {
        let safe_dir = escape_seatbelt_string(sandbox_dir);
        let rules = vec![
            SeatbeltRule::allow("file-read*", Some(&format!("(subpath \"{safe_dir}\")"))),
            SeatbeltRule::allow("file-write*", Some(&format!("(subpath \"{safe_dir}\")"))),
            SeatbeltRule::deny("network*", None),
            SeatbeltRule::deny("mach-lookup", None),
            SeatbeltRule::deny("process-fork", None),
        ];
        Self::new(rules)
    }

    pub fn agent_wallet_readonly(storage_path: &str) -> Self {
        let safe_path = escape_seatbelt_string(storage_path);
        let rules = vec![
            SeatbeltRule::allow("file-read*", Some(&format!("(subpath \"{safe_path}\")"))),
            SeatbeltRule::deny("file-write*", None),
            SeatbeltRule::deny("network*", None),
            SeatbeltRule::deny("process-exec*", None),
        ];
        Self::new(rules)
    }

    pub fn inference_engine_profile(models_path: &str) -> Self {
        let safe_path = escape_seatbelt_string(models_path);
        let rules = vec![
            SeatbeltRule::allow("file-read*", Some(&format!("(subpath \"{safe_path}\")"))),
            SeatbeltRule::allow(
                "mach-lookup",
                Some("(global-name \"com.apple.metal.device\")"),
            ),
            SeatbeltRule::allow(
                "iokit-open",
                Some("(iokit-user-client-class \"AGXCommandQueue\")"),
            ),
            SeatbeltRule::deny("network*", None),
            SeatbeltRule::deny("process-exec*", None),
        ];
        Self::new(rules)
    }

    pub fn validate_safety(&self) -> Result<(), String> {
        if !self.profile_string.contains("(version 1)") {
            return Err("Seatbelt profile must specify (version 1)".to_string());
        }
        if !self.profile_string.contains("(deny default)") {
            return Err("Seatbelt profile must enforce default deny".to_string());
        }
        for key in self.parameters.keys() {
            if !is_valid_seatbelt_ident(key) {
                return Err(format!(
                    "identifiant de paramètre Seatbelt invalide : '{key}'"
                ));
            }
        }
        for rule in &self.rules {
            if !matches!(rule.action.as_str(), "allow" | "deny") {
                return Err(format!("action Seatbelt invalide : '{}'", rule.action));
            }
            if !is_valid_seatbelt_ident(&rule.filter) {
                return Err(format!("filtre Seatbelt invalide : '{}'", rule.filter));
            }
        }
        Ok(())
    }
}
