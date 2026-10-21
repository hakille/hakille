pub mod capability;
pub mod gui_agent;
pub mod protocol;
pub mod reference;
pub mod registry;
pub mod runtime;
pub mod sandbox;
pub mod wasm_runner;

pub use capability::*;
pub use gui_agent::*;
pub use protocol::*;
pub use reference::*;
pub use registry::*;
pub use runtime::*;
pub use sandbox::*;
pub use wasm_runner::*;

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_core::CapabilityScope;

    #[test]
    fn test_agent_registry_lifecycle() {
        let mut registry = AgentRegistry::new();
        assert_eq!(registry.list_agents().len(), 0);

        let manifest = AgentManifest {
            id: "agent-test-mail".to_string(),
            name: "Mail Agent".to_string(),
            version: "0.1.0".to_string(),
            description: "Sandboxed mail client".to_string(),
            required_capabilities: vec![CapabilityScope::Network, CapabilityScope::Filesystem],
        };

        registry
            .register(manifest)
            .expect("Registration should succeed");
        assert_eq!(registry.list_agents().len(), 1);
        assert_eq!(
            registry.get_status("agent-test-mail"),
            Some(AgentStatus::Installed)
        );

        // State transition to Running
        assert!(registry.set_status("agent-test-mail", AgentStatus::Running));
        assert_eq!(
            registry.get_status("agent-test-mail"),
            Some(AgentStatus::Running)
        );

        // State transition to Terminated
        assert!(registry.set_status("agent-test-mail", AgentStatus::Terminated));
        assert_eq!(
            registry.get_status("agent-test-mail"),
            Some(AgentStatus::Terminated)
        );
    }

    #[test]
    fn test_uitars_gui_agent_intent_prediction() {
        let agent = UiTarsAgent::new();

        // 1. Click task
        let click_intents = agent.predict_intent_from_task("Click on the Save button");
        assert_eq!(click_intents.len(), 1);
        match &click_intents[0] {
            GuiIntention::Click { x, y, button } => {
                assert_eq!(*x, 0.5);
                assert_eq!(*y, 0.5);
                assert_eq!(*button, hakille_substrate::MouseButton::Left);
            }
            _ => panic!("Expected Click intent"),
        }

        // 2. Typing task
        let type_intents = agent.predict_intent_from_task("Write report draft");
        assert_eq!(type_intents.len(), 1);
        match &type_intents[0] {
            GuiIntention::Type { text } => {
                assert!(!text.is_empty());
            }
            _ => panic!("Expected Type intent"),
        }
    }

    #[test]
    fn test_inter_agent_signed_message_protocol() {
        let sender_keys = hakille_core::KeyPair::generate();
        let other_keys = hakille_core::KeyPair::generate();

        let msg = InterAgentMessage::sign_and_create(
            "agent-mail",
            "agent-calendar",
            "ScheduleMeeting",
            serde_json::json!({ "title": "Architecture Review", "time": "14:00" }),
            &sender_keys,
        );

        // Champs anti-rejeu/audience renseignés par défaut.
        assert_eq!(msg.audience, "agent-calendar");
        assert_eq!(msg.nonce.len(), 16);
        assert!(!msg.is_expired());

        // Verification with sender's key succeeds
        assert!(msg.verify_signature(&sender_keys.verifying_key));
        assert!(msg.verify_for_audience(&sender_keys.verifying_key, "agent-calendar"));

        // Wrong audience is rejected
        assert!(!msg.verify_for_audience(&sender_keys.verifying_key, "agent-mail"));

        // Verification with another agent's key fails
        assert!(!msg.verify_signature(&other_keys.verifying_key));

        // Tampering with payload fails
        let mut tampered_msg = msg.clone();
        tampered_msg.payload = serde_json::json!({ "title": "Malicious Meeting", "time": "14:00" });
        assert!(!tampered_msg.verify_signature(&sender_keys.verifying_key));

        // Expired message is rejected even with the right key
        let expired = InterAgentMessage::sign_and_create_full(
            "agent-mail",
            "agent-calendar",
            "agent-calendar",
            "ScheduleMeeting",
            serde_json::json!({ "title": "Old", "time": "09:00" }),
            &sender_keys,
            hakille_core::Timestamp(chrono::Utc::now() - chrono::Duration::seconds(60)),
            uuid::Uuid::now_v7().as_bytes().to_vec(),
        );
        assert!(expired.is_expired());
        assert!(!expired.verify_signature(&sender_keys.verifying_key));
    }

    #[test]
    fn test_capability_quota_enforcement_and_auto_revocation() {
        let quota = CapabilityQuota {
            max_invocations_per_minute: 3,
            max_total_invocations: 5,
            max_payload_bytes_per_call: 1024,
            policy_on_breach: QuotaBreachPolicy::AutoRevoke,
        };
        let mut tracker = AgentQuotaTracker::new(quota);

        // 3 calls within 1 minute succeed
        assert!(tracker.record_invocation(256).is_ok());
        assert!(tracker.record_invocation(256).is_ok());
        assert!(tracker.record_invocation(256).is_ok());
        assert_eq!(tracker.total_calls(), 3);

        // 4th call within the same minute violates rate-limit and triggers AutoRevoke (§31c)
        let rate_breach = tracker.record_invocation(256);
        assert!(rate_breach.is_err());
        assert!(tracker.is_revoked());

        // Subsequent call is blocked due to automatic revocation
        let subsequent = tracker.record_invocation(256);
        assert!(subsequent.is_err());
    }

    #[tokio::test]
    async fn test_wasm_runtime_limits_enforced() {
        let runtime = WasmAgentRuntime::new().unwrap();
        let limits = SandboxLimits {
            max_memory_bytes: 1024 * 1024,
            max_fuel: 1000,
            max_execution_time_ms: 500,
            allow_network: false,
            allow_filesystem: false,
            egress_allowlist: Vec::new(),
        };
        let minimal_wasm = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let res = runtime.execute_wasm(minimal_wasm, &limits).await;
        assert_eq!(res.unwrap(), 0);
        // Chaque exécution est comptée dans le quota (§31c).
        assert_eq!(runtime.quota_total_calls(), 1);
        assert!(!runtime.quota_revoked());
    }

    #[tokio::test]
    async fn test_wasm_runtime_quota_breach_blocks_and_revokes() {
        let runtime = WasmAgentRuntime::new().unwrap();
        runtime.set_quota(CapabilityQuota {
            max_invocations_per_minute: 1,
            max_total_invocations: 10,
            max_payload_bytes_per_call: 1024,
            policy_on_breach: QuotaBreachPolicy::AutoRevoke,
        });
        let limits = SandboxLimits::default();
        let minimal_wasm = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

        assert!(runtime.execute_wasm(minimal_wasm, &limits).await.is_ok());
        // 2e appel dans la minute : refus + AutoRevoke.
        assert!(runtime.execute_wasm(minimal_wasm, &limits).await.is_err());
        assert!(runtime.quota_revoked());
        // Les appels suivants restent bloqués.
        assert!(runtime.execute_wasm(minimal_wasm, &limits).await.is_err());
    }

    #[test]
    fn test_verify_attenuation_compares_parent_and_child() {
        use hakille_core::{CapabilityAction, CapabilityAuthority, Timestamp};

        let verifier = AgentCapabilityVerifier::new(CapabilityAuthority::new());
        let authority = &verifier.authority;
        let expiry_parent = Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600));
        let parent = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem, CapabilityScope::Network],
                vec![CapabilityAction::Read, CapabilityAction::Write],
                expiry_parent.clone(),
            )
            .unwrap();

        // Atténuation légitime : sous-ensemble + expiry plus courte.
        let child = authority
            .attenuate(
                &parent,
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                Timestamp(chrono::Utc::now() + chrono::Duration::seconds(600)),
            )
            .unwrap();
        assert!(
            verifier
                .verify_attenuation(
                    &parent,
                    &child,
                    CapabilityScope::Filesystem,
                    CapabilityAction::Read,
                )
                .is_ok()
        );
        // Hors sous-ensemble : refus.
        assert!(
            verifier
                .verify_attenuation(
                    &parent,
                    &child,
                    CapabilityScope::Network,
                    CapabilityAction::Read,
                )
                .is_err()
        );

        // Escalade : enfant forgé depuis une autre racine, même contenu.
        let rogue = CapabilityAuthority::new();
        let rogue_parent = rogue
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600)),
            )
            .unwrap();
        let rogue_child = rogue
            .attenuate(
                &rogue_parent,
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                Timestamp(chrono::Utc::now() + chrono::Duration::seconds(600)),
            )
            .unwrap();
        assert!(
            verifier
                .verify_attenuation(
                    &parent,
                    &rogue_child,
                    CapabilityScope::Filesystem,
                    CapabilityAction::Read,
                )
                .is_err()
        );

        // Expiry enfant > parent : refus (fail-closed dès `attenuate`
        // depuis le durcissement monotonie temporelle, sinon au `verify`).
        let long_child_res = verifier.authority.attenuate(
            &parent,
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            Timestamp(chrono::Utc::now() + chrono::Duration::seconds(7200)),
        );
        match long_child_res {
            Err(_) => {} // Refus précoce attendu (monotonie temporelle).
            Ok(long_child) => {
                assert!(
                    verifier
                        .verify_attenuation(
                            &parent,
                            &long_child,
                            CapabilityScope::Filesystem,
                            CapabilityAction::Read,
                        )
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn test_replay_guard_rejects_duplicate_and_purges() {
        let kp = hakille_core::KeyPair::generate();
        let msg = InterAgentMessage::sign_and_create("a", "b", "Ping", serde_json::json!({}), &kp);
        let mut guard = ReplayGuard::new();
        // 1er passage : accepté.
        assert!(
            msg.verify_signature_with_guard(&kp.verifying_key, &mut guard)
                .unwrap()
        );
        assert_eq!(guard.len(), 1);
        // Doublon dans la fenêtre 5 min : Err (rejeu).
        let dup = msg.verify_signature_with_guard(&kp.verifying_key, &mut guard);
        assert!(dup.is_err());
        // Signature invalide → Ok(false), garde non incrémentée.
        let other = hakille_core::KeyPair::generate();
        let mut guard2 = ReplayGuard::new();
        assert!(
            !msg.verify_signature_with_guard(&other.verifying_key, &mut guard2)
                .unwrap()
        );
        assert!(guard2.is_empty());
    }

    #[test]
    fn test_registry_verified_hash_duplicate_and_readonly() {
        let mut reg = AgentRegistry::new();
        let mk = |ver: &str| AgentManifest {
            id: "agent-x".to_string(),
            name: "X".to_string(),
            version: ver.to_string(),
            description: "d".to_string(),
            required_capabilities: vec![],
        };
        // Version invalide refusée.
        assert!(reg.register_verified(mk("bad"), None, false).is_err());
        assert!(reg.register_verified(mk(""), None, false).is_err());
        // Enregistrement ok.
        let m = mk("0.1.0");
        let h = AgentRegistry::manifest_hash(&m);
        reg.register_verified(m.clone(), Some(h), false).unwrap();
        assert_eq!(reg.get_expected_hash("agent-x"), Some(h));
        // Doublon sans force refusé, avec force accepté.
        assert!(reg.register_verified(m.clone(), None, false).is_err());
        reg.register_verified(m.clone(), None, true).unwrap();
        // Hash attendu non conforme refusé.
        assert!(reg.register_verified(m, Some([9u8; 32]), true).is_err());
        // read_only bloque register + set_status.
        reg.set_read_only(true);
        assert!(reg.check().is_err());
        let m2 = AgentManifest {
            id: "agent-y".to_string(),
            name: "Y".to_string(),
            version: "1.0.0".to_string(),
            description: "d".to_string(),
            required_capabilities: vec![],
        };
        assert!(reg.register(m2).is_err());
        assert!(!reg.set_status("agent-x", AgentStatus::Running));
        assert!(
            reg.set_status_checked("agent-x", AgentStatus::Running)
                .is_err()
        );
        reg.set_read_only(false);
        assert!(reg.check().is_ok());
    }

    #[test]
    fn test_gui_clamp_screen0_and_sensitive() {
        let agent = UiTarsAgent::new();
        // Clamp x,y hors [0,1].
        let click = GuiIntention::Click {
            x: 2.5,
            y: -1.0,
            button: hakille_substrate::MouseButton::Left,
        };
        let act = agent
            .to_input_action_checked(&click, 1000, 800, true)
            .unwrap()
            .unwrap();
        match act {
            hakille_substrate::InputAction::Click { x, y, .. } => {
                assert_eq!(x, 1000);
                assert_eq!(y, 0);
            }
            _ => panic!("click attendu"),
        }
        // Écran 0 → Err (variante stricte), None (compat).
        assert!(agent.to_input_action_checked(&click, 0, 800, true).is_err());
        assert!(agent.to_input_action(&click, 0, 800).is_none());
        // Type sensible sans confirm → Err.
        let sensitive = GuiIntention::Type {
            text: "my Password is 123".to_string(),
        };
        assert!(
            agent
                .to_input_action_checked(&sensitive, 800, 600, false)
                .is_err()
        );
        assert!(
            agent
                .to_input_action_checked(&sensitive, 800, 600, true)
                .is_ok()
        );
        // Hotkey sans confirm → Err.
        let hk = GuiIntention::Hotkey { keys: vec![] };
        assert!(agent.to_input_action_checked(&hk, 800, 600, false).is_err());
        assert!(agent.to_input_action_checked(&hk, 800, 600, true).is_ok());
    }

    #[tokio::test]
    async fn test_wasm_runtime_quota_payload_and_total_breach() {
        let runtime = WasmAgentRuntime::new().unwrap();
        // payload_max très petit : tout module > 8 octets est refusé + révoqué.
        runtime.set_quota(CapabilityQuota {
            max_invocations_per_minute: 100,
            max_total_invocations: 100,
            max_payload_bytes_per_call: 8,
            policy_on_breach: QuotaBreachPolicy::AutoRevoke,
        });
        let limits = SandboxLimits::default();
        let big_wasm = vec![0u8; 64];
        assert!(runtime.execute_wasm(&big_wasm, &limits).await.is_err());
        assert!(runtime.quota_revoked());

        // Quota total : 2 appels autorisés puis blocage + révocation.
        let runtime2 = WasmAgentRuntime::new().unwrap();
        runtime2.set_quota(CapabilityQuota {
            max_invocations_per_minute: 100,
            max_total_invocations: 2,
            max_payload_bytes_per_call: 1024,
            policy_on_breach: QuotaBreachPolicy::AutoRevoke,
        });
        let minimal_wasm = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        assert!(runtime2.execute_wasm(minimal_wasm, &limits).await.is_ok());
        assert!(runtime2.execute_wasm(minimal_wasm, &limits).await.is_ok());
        assert_eq!(runtime2.quota_total_calls(), 2);
        assert!(runtime2.execute_wasm(minimal_wasm, &limits).await.is_err());
        assert!(runtime2.quota_revoked());
    }

    #[test]
    fn test_sandbox_egress_allowlist_fail_closed() {
        let denied = SandboxLimits::default();
        assert!(!denied.is_egress_allowed("10.0.0.1"));

        let with_net_no_list = SandboxLimits {
            allow_network: true,
            ..SandboxLimits::default()
        };
        // allow_network seul ne suffit pas : allowlist vide = tout refusé.
        assert!(!with_net_no_list.is_egress_allowed("10.0.0.1"));

        let allowed = SandboxLimits {
            allow_network: true,
            egress_allowlist: vec!["10.0.0.1".to_string()],
            ..SandboxLimits::default()
        };
        assert!(allowed.is_egress_allowed("10.0.0.1"));
        assert!(!allowed.is_egress_allowed("10.0.0.2"));
    }

    #[tokio::test]
    async fn test_wasm_wasi_preview1_linked_and_unknown_import_rejected() {
        let runtime = WasmAgentRuntime::new().unwrap();
        let limits = SandboxLimits::default();

        // Import inconnu (`env::unknown`) → échec d'instanciation fail-closed.
        let unknown_wat = r#"(module (import "env" "unknown" (func)))"#;
        let unknown_wasm = wat::parse_str(unknown_wat).unwrap();
        let err = runtime.execute_wasm(&unknown_wasm, &limits).await;
        assert!(err.is_err(), "import inconnu doit être rejeté fail-closed");

        // Module WASI preview1 (`fd_write`) s'instancie via le linker
        // `wasi_snapshot_preview1` (exécution `run` → 0, pas de réseau réel).
        let wasi_wat = r#"(module
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (memory 1)
            (func (export "run") (result i32) i32.const 0))"#;
        let wasi_wasm = wat::parse_str(wasi_wat).unwrap();
        let ok = runtime.execute_wasm(&wasi_wasm, &limits).await;
        assert_eq!(ok.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_wasm_fuel_traps_infinite_loop() {
        let runtime = WasmAgentRuntime::new().unwrap();
        let limits = SandboxLimits {
            max_fuel: 500,
            ..SandboxLimits::default()
        };
        // Boucle infinie : le fuel doit piéger (fail-closed, pas de hang).
        let loop_wat = r#"(module (func (export "run") (result i32)
            (loop (br 0)) i32.const 0))"#;
        let loop_wasm = wat::parse_str(loop_wat).unwrap();
        assert!(runtime.execute_wasm(&loop_wasm, &limits).await.is_err());
    }

    #[tokio::test]
    async fn test_wasm_fs_sandbox_temp_dir_lifecycle() {
        let runtime = WasmAgentRuntime::new().unwrap();
        // Sans FS : exécution OK, aucun préouvert (le module WASI sans accès
        // fichier ne peut pas sortir du sandbox).
        let limits_no_fs = SandboxLimits::default();
        let wasi_wat = r#"(module (func (export "run") (result i32) i32.const 7))"#;
        let wasm = wat::parse_str(wasi_wat).unwrap();
        assert_eq!(runtime.execute_wasm(&wasm, &limits_no_fs).await.unwrap(), 7);

        // Avec FS : répertoire temp dédié pré-ouvert en `/sandbox`, nettoyé
        // après exécution (best-effort, pas d'erreur même si déjà supprimé).
        let limits_fs = SandboxLimits {
            allow_filesystem: true,
            ..SandboxLimits::default()
        };
        assert_eq!(runtime.execute_wasm(&wasm, &limits_fs).await.unwrap(), 7);
    }

    #[test]
    fn test_registry_version_incompatible_rejected() {
        // Format invalide toujours rejeté.
        assert!(AgentRegistry::validate_version("bad").is_err());
        assert!(AgentRegistry::validate_version("1").is_err());
        assert!(AgentRegistry::validate_version("").is_err());
        assert!(AgentRegistry::validate_version("0.1.0").is_ok());

        // Compatibilité caret : même majeur + >= min.
        assert!(AgentRegistry::check_version_compatible("2.1.0", "2.0.0").is_ok());
        assert!(AgentRegistry::check_version_compatible("2.0.0", "2.0.0").is_ok());
        // Majeur différent → rejetée.
        assert!(AgentRegistry::check_version_compatible("1.9.9", "2.0.0").is_err());
        assert!(AgentRegistry::check_version_compatible("3.0.0", "2.0.0").is_err());
        // Mineur/patch inférieur → rejetée.
        assert!(AgentRegistry::check_version_compatible("2.0.0", "2.1.0").is_err());
        assert!(AgentRegistry::check_version_compatible("2.0.1", "2.0.2").is_err());
    }
}
