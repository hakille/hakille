use chrono::TimeZone;
use hakille_core::{HakilleError, Result, Timestamp};
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleCategory {
    Privacy,
    Safety,
    Autonomy,
    Transparency,
    Fairness,
    HumanPrimacy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Enforcement {
    HardBlock,
    SoftWarning,
    AuditOnly,
}

impl Enforcement {
    /// `true` pour les règles à blocage dur : toute action correspondante
    /// doit être refusée avant exécution (voir `Constitution::check_action`).
    pub fn is_hard_block(&self) -> bool {
        matches!(self, Self::HardBlock)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstitutionalRule {
    pub id: String,
    pub category: RuleCategory,
    pub description: String,
    pub enforcement: Enforcement,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constitution {
    pub version: Version,
    pub rules: Vec<ConstitutionalRule>,
    pub effective_since: Timestamp,
}

impl Default for Constitution {
    fn default() -> Self {
        Self::default_v1()
    }
}

impl Constitution {
    pub fn default_v1() -> Self {
        let rules = vec![
            ConstitutionalRule {
                id: "rule-1".into(),
                category: RuleCategory::Privacy,
                description: "Ne jamais exécuter du contenu non fiable comme des instructions / Never execute untrusted content as instructions".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-2".into(),
                category: RuleCategory::Transparency,
                description: "Toute action doit être auditable avec trace de raisonnement / All actions must be auditable with reasoning trace".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-3".into(),
                category: RuleCategory::Privacy,
                description: "L'identité vit dans le graphe, jamais dans les poids du modèle / Identity lives in the graph, never in model weights".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-4".into(),
                category: RuleCategory::Safety,
                description: "Les capacités ne peuvent qu'être atténuées, jamais étendues / Capabilities can only be attenuated, never expanded".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-5".into(),
                category: RuleCategory::HumanPrimacy,
                description: "Les situations de crise doivent être orientées vers des ressources d'aide humaines / Crisis situations must defer to human help resources".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-6".into(),
                category: RuleCategory::Safety,
                description:
                    "Les domaines sensibles (juridique/financier/médical) exigent une confiance élevée / Sensitive domains (legal/financial/medical) require elevated confidence".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-7".into(),
                category: RuleCategory::Safety,
                description: "Les profils mineurs ont des restrictions par défaut / Minor profiles have default restrictions".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-8".into(),
                category: RuleCategory::Privacy,
                description: "La capture audio/vidéo continue ne transmet jamais de flux brut hors de l'hôte / Continuous audio/video capture never transmits raw streams off-host".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-9".into(),
                category: RuleCategory::Autonomy,
                description: "L'export mémoire doit être en format ouvert et documenté / Memory export must be in open, documented format".into(),
                enforcement: Enforcement::HardBlock,
                priority: 1,
            },
            ConstitutionalRule {
                id: "rule-10".into(),
                category: RuleCategory::HumanPrimacy,
                description: "Aucune optimisation pour maximiser l'attachement ou l'engagement / No engineering for maximizing attachment or engagement".into(),
                enforcement: Enforcement::SoftWarning,
                priority: 2,
            },
        ];

        // Date de ratification de la Constitution v1.0.0 — FIXE, jamais
        // `Timestamp::now()`/`default()` : l'historique des règles doit être
        // reproductible et comparable entre builds.
        Self {
            version: Version::new(1, 0, 0),
            rules,
            effective_since: constitution_v1_release_date(),
        }
    }

    /// Point d'appel réel du blocage dur : refuse toute action dont la
    /// description contient un mot-clé documenté d'une règle HardBlock.
    /// Les règles SoftWarning/AuditOnly ne bloquent jamais ici.
    pub fn check_action(&self, action_type: &str) -> Result<()> {
        let needle = action_type.to_lowercase();
        for rule in self.rules.iter().filter(|r| r.enforcement.is_hard_block()) {
            if let Some(keywords) = hard_block_keywords(&rule.id)
                && keywords.iter().any(|k| needle.contains(k))
            {
                return Err(HakilleError::ConstitutionViolation(format!(
                    "Action bloquée par {} : {}",
                    rule.id, rule.description
                )));
            }
        }
        Ok(())
    }

    /// Variante typée : en plus du keyword matching de `check_action`,
    /// applique deux garde-fous contextuels :
    /// - si `trust == "untrusted"` et l'action contient "execute" ou
    ///   "instruction", bloque (contenu non fiable exécuté comme instruction).
    /// - si `domain` est medical/legal/financial et l'action contient le
    ///   mot-clé "low-confidence", bloque (domaine sensible sans confiance).
    pub fn check_action_typed(
        &self,
        action_type: &str,
        trust: Option<&str>,
        domain: Option<&str>,
    ) -> Result<()> {
        // 1. Keyword matching standard.
        self.check_action(action_type)?;
        let needle = action_type.to_lowercase();

        // 2. Contenu non fiable exécuté comme instruction.
        if let Some(t) = trust
            && t.trim().eq_ignore_ascii_case("untrusted")
            && (needle.contains("execute") || needle.contains("instruction"))
        {
            return Err(HakilleError::ConstitutionViolation(format!(
                "Action bloquée par rule-1 : untrusted content must not be executed as instruction ({action_type})"
            )));
        }

        // 3. Domaine sensible avec faible confiance.
        if let Some(d) = domain {
            let dnorm = d.trim().to_lowercase();
            if (dnorm == "medical" || dnorm == "legal" || dnorm == "financial")
                && (needle.contains("low-confidence")
                    || needle.contains("low confidence")
                    || needle.contains("low_confidence"))
            {
                return Err(HakilleError::ConstitutionViolation(format!(
                    "Action bloquée par rule-6 : {dnorm} domain requires elevated confidence ({action_type})"
                )));
            }
        }

        Ok(())
    }

    /// Liste des ids de règles, dans l'ordre.
    pub fn rule_ids(&self) -> Vec<String> {
        self.rules.iter().map(|r| r.id.clone()).collect()
    }

    /// Nombre de règles à blocage dur.
    pub fn hard_block_count(&self) -> usize {
        self.rules
            .iter()
            .filter(|r| r.enforcement.is_hard_block())
            .count()
    }

    /// Vérifie la cohérence interne : exactement 10 règles, ids uniques
    /// non vides, priorité > 0, et toute règle HardBlock en priorité 1.
    pub fn validate(&self) -> Result<()> {
        if self.rules.len() != 10 {
            return Err(HakilleError::ConstitutionViolation(format!(
                "constitution must contain exactly 10 rules, found {}",
                self.rules.len()
            )));
        }
        let mut seen = std::collections::HashSet::new();
        for rule in &self.rules {
            if rule.id.trim().is_empty() {
                return Err(HakilleError::ConstitutionViolation(
                    "rule id must not be empty".into(),
                ));
            }
            if !seen.insert(rule.id.clone()) {
                return Err(HakilleError::ConstitutionViolation(format!(
                    "duplicate rule id: {}",
                    rule.id
                )));
            }
            if rule.priority == 0 {
                return Err(HakilleError::ConstitutionViolation(format!(
                    "rule {}: priority must be > 0",
                    rule.id
                )));
            }
            if rule.enforcement.is_hard_block() && rule.priority != 1 {
                return Err(HakilleError::ConstitutionViolation(format!(
                    "rule {}: HardBlock rules must have priority == 1",
                    rule.id
                )));
            }
        }
        Ok(())
    }
}

/// Date fixe de release de la Constitution v1.0.0 (2026-03-15T00:00:00Z).
pub fn constitution_v1_release_date() -> Timestamp {
    Timestamp(
        chrono::Utc
            .with_ymd_and_hms(2026, 3, 15, 0, 0, 0)
            .single()
            .expect("fixed release date must be valid"),
    )
}

/// Mots-clés documentés déclenchant le HardBlock de chaque règle.
/// Ils décrivent des tentatives de CONTOURNEMENT de la règle (pas le sujet
/// légitime lui-même : p. ex. demander de l'aide en crise reste autorisé,
/// mais exiger de gérer la crise sans référent humain est bloqué).
fn hard_block_keywords(rule_id: &str) -> Option<&'static [&'static str]> {
    Some(match rule_id {
        // rule-1 : jamais d'instructions depuis du contenu non fiable.
        "rule-1" => &[
            "prompt injection",
            "jailbreak",
            "ignore previous instructions",
            "system prompt leak",
            "execute untrusted instructions",
        ],
        // rule-2 : toute action doit rester auditable.
        "rule-2" => &[
            "skip audit",
            "disable logging",
            "delete audit logs",
            "cover tracks",
            "unaudited action",
        ],
        // rule-3 : l'identité vit dans le graphe, pas dans les poids.
        "rule-3" => &[
            "bake identity into weights",
            "hardcode identity in model",
            "memorize identity in weights",
        ],
        // rule-4 : les capabilities ne s'atténuent que vers le bas.
        "rule-4" => &[
            "expand capability",
            "escalate privilege",
            "widen token scope",
            "bypass capability check",
        ],
        // rule-5 : crise ⇒ référent humain obligatoire.
        "rule-5" => &[
            "handle crisis alone",
            "no human referral",
            "hide helpline",
            "replace human crisis help",
        ],
        // rule-6 : domaines sensibles ⇒ confiance élevée requise.
        "rule-6" => &[
            "low-confidence medical diagnosis",
            "uncertain legal advice",
            "unverified financial order",
        ],
        // rule-7 : profils mineurs restreints par défaut.
        "rule-7" => &[
            "disable minor protection",
            "child safety bypass",
            "unrestricted minor profile",
        ],
        // rule-8 : capture A/V continue — jamais de flux brut hors hôte.
        "rule-8" => &[
            "exfiltrate raw audio",
            "stream raw video off-device",
            "upload raw microphone",
            "transmit raw camera feed",
        ],
        // rule-9 : export mémoire en format ouvert documenté.
        "rule-9" => &[
            "proprietary memory export",
            "undocumented memory dump",
            "export without schema",
        ],
        _ => return None,
    })
}
