//! Tests empiriques et adversariaux pour `hakille-ui-protocol` (Jalon M1).
//!
//! Vérifie :
//! 1. La sérialisation / désérialisation de payloads malformés (JSON corrompu, types erronés, champs manquants, versions obsolètes/futures).
//! 2. La robustesse exhaustive des énumérations (variantes valides, variantes inconnues, sensibilité à la casse, tags malformés).
//! 3. La cohérence mathématique et opérationnelle des masques binaires `PERCEPTION_CHANNEL_*`.

use hakille_ui_protocol::*;

// ============================================================================
// 1. Tests Adversariaux : Sérialisation et Désérialisation de Payloads Malformés
// ============================================================================

#[test]
fn test_malformed_json_syntax_rejection() {
    let malformed_syntaxes = [
        "",                                                  // vide
        "   ",                                               // espaces
        "{",                                                 // JSON tronqué
        "{\"version\": 1, \"inner\": {",                     // objet tronqué
        "{\"version\": 1, \"inner\": null}",                 // inner null
        "{\"version\": 1, \"inner\": 42}",                   // type inattendu
        "{\"version\": \"1\", \"inner\": \"OutboxFlush\"}",  // version sous forme de string
        "{\"version\": 1, \"inner\": {\"Approve\": {}}}",    // card_id manquant
        "[1, 2, 3]",                                        // tableau au lieu d'objet
        "\0\0\0\0",                                          // octets nuls
    ];

    for bad in &malformed_syntaxes {
        let res_ui: Result<UiMessage, _> = serde_json::from_str(bad);
        assert!(res_ui.is_err(), "UiMessage doit rejeter le payload malformé : {bad:?}");

        let res_kernel: Result<KernelMessage, _> = serde_json::from_str(bad);
        assert!(res_kernel.is_err(), "KernelMessage doit rejeter le payload malformé : {bad:?}");
    }
}

#[test]
fn test_type_mismatches_and_missing_mandatory_fields() {
    // 1. Manque le champ obligatoire 'active_channels_mask' dans PerceptionControl
    let missing_mask = r#"{
        "version": 1,
        "inner": {
            "PerceptionControl": {
                "hardware_camera_enabled": true,
                "hardware_mic_enabled": true,
                "cognitive_inference_enabled": true
            }
        }
    }"#;
    assert!(serde_json::from_str::<UiMessage>(missing_mask).is_err());

    // 2. Type erroné pour 'active_channels_mask' (string au lieu de u32)
    let bad_mask_type = r#"{
        "version": 1,
        "inner": {
            "PerceptionControl": {
                "hardware_camera_enabled": true,
                "hardware_mic_enabled": true,
                "cognitive_inference_enabled": true,
                "active_channels_mask": "all"
            }
        }
    }"#;
    assert!(serde_json::from_str::<UiMessage>(bad_mask_type).is_err());

    // 3. Type négatif pour 'active_channels_mask' (-1 pour u32)
    let negative_mask = r#"{
        "version": 1,
        "inner": {
            "PerceptionControl": {
                "hardware_camera_enabled": true,
                "hardware_mic_enabled": true,
                "cognitive_inference_enabled": true,
                "active_channels_mask": -1
            }
        }
    }"#;
    assert!(serde_json::from_str::<UiMessage>(negative_mask).is_err());

    // 4. Manque le champ obligatoire 'severity' dans FatigueAlertEvent
    let missing_severity = r#"{
        "version": 1,
        "inner": {
            "FatigueAlert": {
                "perclos_p80": 0.15,
                "ear": 0.20,
                "mar": 0.50,
                "yawn_count": 2,
                "recommendation": "pause",
                "timestamp_ms": 1000
            }
        }
    }"#;
    assert!(serde_json::from_str::<KernelMessage>(missing_severity).is_err());

    // 5. Type erroné pour 'timestamp_ms' (string au lieu d'entier)
    let bad_timestamp = r#"{
        "version": 1,
        "inner": {
            "FatigueAlert": {
                "severity": "Mild",
                "perclos_p80": 0.15,
                "ear": 0.20,
                "mar": 0.50,
                "yawn_count": 2,
                "recommendation": "pause",
                "timestamp_ms": "2026-11-06T19:00:00Z"
            }
        }
    }"#;
    assert!(serde_json::from_str::<KernelMessage>(bad_timestamp).is_err());
}

#[test]
fn test_version_negotiation_clean_rejection() {
    let invalid_versions = [0, 2, 3, 10, 100, 65535];

    for v in invalid_versions {
        assert!(!compatible(v as u32), "compatible({v}) doit être faux");

        let msg_ui = UiMessage {
            version: v,
            inner: UiToKernel::OutboxFlush,
        };
        let check_res = msg_ui.check();
        assert!(check_res.is_err(), "UiMessage v{v} doit être rejeté par .check()");
        let err_msg = check_res.unwrap_err();
        assert!(err_msg.contains("version protocole incompatible"));
        assert!(err_msg.contains(&v.to_string()));

        let msg_kernel = KernelMessage {
            version: v,
            inner: KernelToUi::Presence(PresenceState::Idle),
        };
        assert!(msg_kernel.check().is_err(), "KernelMessage v{v} doit être rejeté par .check()");
    }

    // Version courante v1
    let ok_ui = UiMessage::new(UiToKernel::OutboxFlush);
    assert_eq!(ok_ui.version, PROTOCOL_VERSION);
    assert!(ok_ui.check().is_ok());

    let ok_kernel = KernelMessage::new(KernelToUi::Presence(PresenceState::Idle));
    assert_eq!(ok_kernel.version, PROTOCOL_VERSION);
    assert!(ok_kernel.check().is_ok());
}

// ============================================================================
// 2. Tests Adversariaux : Robustesse des Énumérations
// ============================================================================

#[test]
fn test_enum_unknown_variants_and_casing_rejection() {
    // 1. UserPresenceKind
    assert!(serde_json::from_str::<UserPresenceKind>(r#""Sleeping""#).is_err());
    assert!(serde_json::from_str::<UserPresenceKind>(r#""present""#).is_err()); // minuscule refusée
    assert!(serde_json::from_str::<UserPresenceKind>(r#""PRESENT""#).is_err());
    assert!(serde_json::from_str::<UserPresenceKind>(r#"1"#).is_err());

    // 2. AttentionKind
    assert!(serde_json::from_str::<AttentionKind>(r#""Daydreaming""#).is_err());
    assert!(serde_json::from_str::<AttentionKind>(r#""focused""#).is_err());
    assert!(serde_json::from_str::<AttentionKind>(r#"null"#).is_err());

    // 3. PostureKindDto
    assert!(serde_json::from_str::<PostureKindDto>(r#""LyingDown""#).is_err());
    assert!(serde_json::from_str::<PostureKindDto>(r#""upright""#).is_err());
    assert!(serde_json::from_str::<PostureKindDto>(r#"{}"#).is_err());

    // 4. FatigueSeverity
    assert!(serde_json::from_str::<FatigueSeverity>(r#""DeadTired""#).is_err());
    assert!(serde_json::from_str::<FatigueSeverity>(r#""mild""#).is_err());
    assert!(serde_json::from_str::<FatigueSeverity>(r#"false"#).is_err());

    // 5. PrivacyGuardTrigger
    assert!(serde_json::from_str::<PrivacyGuardTrigger>(r#""UnknownAlarm""#).is_err());
    assert!(serde_json::from_str::<PrivacyGuardTrigger>(r#"{"ShoulderSurfingDetected": {"people_count": "two"}}"#).is_err());
}

#[test]
fn test_exhaustive_roundtrip_all_enums_and_variants() {
    // UserPresenceKind (4 variantes)
    let presences = [
        UserPresenceKind::Present,
        UserPresenceKind::Absent,
        UserPresenceKind::Distracted,
        UserPresenceKind::MultiplePeople,
    ];
    for p in presences {
        let json = serde_json::to_string(&p).unwrap();
        let back: UserPresenceKind = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    // AttentionKind (4 variantes)
    let attentions = [
        AttentionKind::Focused,
        AttentionKind::Divided,
        AttentionKind::Distracted,
        AttentionKind::Absent,
    ];
    for a in attentions {
        let json = serde_json::to_string(&a).unwrap();
        let back: AttentionKind = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }

    // PostureKindDto (7 variantes)
    let postures = [
        PostureKindDto::Upright,
        PostureKindDto::Slouched,
        PostureKindDto::LeaningForward,
        PostureKindDto::Reclined,
        PostureKindDto::Standing,
        PostureKindDto::Seated,
        PostureKindDto::Unknown,
    ];
    for post in postures {
        let json = serde_json::to_string(&post).unwrap();
        let back: PostureKindDto = serde_json::from_str(&json).unwrap();
        assert_eq!(post, back);
    }

    // FatigueSeverity (3 variantes)
    let severities = [
        FatigueSeverity::Mild,
        FatigueSeverity::Moderate,
        FatigueSeverity::Critical,
    ];
    for s in severities {
        let json = serde_json::to_string(&s).unwrap();
        let back: FatigueSeverity = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    // PrivacyGuardTrigger (4 variantes)
    let triggers = [
        PrivacyGuardTrigger::ShoulderSurfingDetected { people_count: 3 },
        PrivacyGuardTrigger::RawDataExfiltrationBlocked {
            blocked_scope: "Camera".into(),
        },
        PrivacyGuardTrigger::HardwareKillSwitchActivated {
            device: "Microphone".into(),
        },
        PrivacyGuardTrigger::EphemeralTtlPurged,
    ];
    for t in triggers {
        let json = serde_json::to_string(&t).unwrap();
        let back: PrivacyGuardTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }
}

// ============================================================================
// 3. Tests de Cohérence et Robustesse des Masques Binaires
// ============================================================================

#[test]
fn test_bitmask_consistency_and_independence() {
    let channels = [
        ("VISION", PERCEPTION_CHANNEL_VISION, 1u32 << 0, 1u32),
        ("AUDIO", PERCEPTION_CHANNEL_AUDIO, 1u32 << 1, 2u32),
        ("EMOTION", PERCEPTION_CHANNEL_EMOTION, 1u32 << 2, 4u32),
        ("POSTURE", PERCEPTION_CHANNEL_POSTURE, 1u32 << 3, 8u32),
        ("FATIGUE", PERCEPTION_CHANNEL_FATIGUE, 1u32 << 4, 16u32),
        ("GAZE", PERCEPTION_CHANNEL_GAZE, 1u32 << 5, 32u32),
    ];

    // 1. Vérification des valeurs individuelles
    for (name, const_val, shifted, expected_num) in channels {
        assert_eq!(const_val, shifted, "{name} constante non alignée avec 1 << bit");
        assert_eq!(const_val, expected_num, "{name} doit valoir {expected_num}");
        assert_eq!(const_val.count_ones(), 1, "{name} doit posséder exactement un bit à 1");
    }

    // 2. Vérification de l'indépendance binaire par paires (orthogonalité stricte)
    for i in 0..channels.len() {
        for j in (i + 1)..channels.len() {
            let (name_a, mask_a, _, _) = channels[i];
            let (name_b, mask_b, _, _) = channels[j];
            assert_eq!(
                mask_a & mask_b, 0,
                "Les masques {name_a} (0x{mask_a:02x}) et {name_b} (0x{mask_b:02x}) se chevauchent !"
            );
        }
    }

    // 3. Vérification de la constante PERCEPTION_CHANNEL_ALL
    let union_all = PERCEPTION_CHANNEL_VISION
        | PERCEPTION_CHANNEL_AUDIO
        | PERCEPTION_CHANNEL_EMOTION
        | PERCEPTION_CHANNEL_POSTURE
        | PERCEPTION_CHANNEL_FATIGUE
        | PERCEPTION_CHANNEL_GAZE;

    assert_eq!(
        PERCEPTION_CHANNEL_ALL, union_all,
        "PERCEPTION_CHANNEL_ALL (0x{PERCEPTION_CHANNEL_ALL:02x}) doit valoir l'union exacte (0x{union_all:02x})"
    );
    assert_eq!(PERCEPTION_CHANNEL_ALL, 0x3F); // 63
    assert_eq!(PERCEPTION_CHANNEL_ALL.count_ones(), 6);

    // 4. Test exhaustif des 64 combinaisons de sous-ensembles (2^6)
    for combo in 0u32..64u32 {
        let has_vision = (combo & PERCEPTION_CHANNEL_VISION) != 0;
        let has_audio = (combo & PERCEPTION_CHANNEL_AUDIO) != 0;
        let has_emotion = (combo & PERCEPTION_CHANNEL_EMOTION) != 0;
        let has_posture = (combo & PERCEPTION_CHANNEL_POSTURE) != 0;
        let has_fatigue = (combo & PERCEPTION_CHANNEL_FATIGUE) != 0;
        let has_gaze = (combo & PERCEPTION_CHANNEL_GAZE) != 0;

        assert_eq!(has_vision, (combo & (1 << 0)) != 0);
        assert_eq!(has_audio, (combo & (1 << 1)) != 0);
        assert_eq!(has_emotion, (combo & (1 << 2)) != 0);
        assert_eq!(has_posture, (combo & (1 << 3)) != 0);
        assert_eq!(has_fatigue, (combo & (1 << 4)) != 0);
        assert_eq!(has_gaze, (combo & (1 << 5)) != 0);

        // Test d'effacement / toggle
        let cleared_vision = combo & !PERCEPTION_CHANNEL_VISION;
        assert_eq!(cleared_vision & PERCEPTION_CHANNEL_VISION, 0);

        let set_all = combo | PERCEPTION_CHANNEL_ALL;
        assert_eq!(set_all, PERCEPTION_CHANNEL_ALL);
    }

    // 5. Présence de bits supérieurs hors ALL
    let extra_mask = 0xFFFFFFFF_u32;
    let standard_part = extra_mask & PERCEPTION_CHANNEL_ALL;
    let non_standard_part = extra_mask & !PERCEPTION_CHANNEL_ALL;
    assert_eq!(standard_part, 0x3F);
    assert_eq!(non_standard_part, 0xFFFFFFC0);
}

#[test]
fn test_perception_control_mask_roundtrip_edge_cases() {
    let test_masks = [
        0u32, // aucun canal
        PERCEPTION_CHANNEL_VISION,
        PERCEPTION_CHANNEL_ALL,
        PERCEPTION_CHANNEL_AUDIO | PERCEPTION_CHANNEL_GAZE,
        0x80000000u32, // bit de poids fort
        u32::MAX,      // tous bits à 1
    ];

    for mask in test_masks {
        let cmd = UiToKernel::PerceptionControl {
            hardware_camera_enabled: false,
            hardware_mic_enabled: true,
            cognitive_inference_enabled: true,
            active_channels_mask: mask,
        };

        let msg = UiMessage::new(cmd.clone());
        let json = serde_json::to_string(&msg).unwrap();
        let back: UiMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);

        if let UiToKernel::PerceptionControl {
            active_channels_mask,
            ..
        } = back.inner
        {
            assert_eq!(active_channels_mask, mask);
        } else {
            panic!("Variante inattendue");
        }
    }
}
