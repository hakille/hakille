//! Tests de sécurité adversariaux pour les capabilities Biscuit v5 dans `hakille-core`.
//!
//! Vérifie empiriquement l'inviolabilité des tokens atténués face aux attaques :
//! - Tentatives d'accès aux flux capteurs et biométriques bruts avec un token atténué
//! - Tentatives d'escalade de privilèges horizontaux (ressources cognitives non autorisées)
//! - Falsification cryptographique de token (mauvaise clé racine épinglée)
//! - Altération et falsification de bytes / signatures
//! - Tentatives d'extension temporelle (violation de monotonie)
//! - Tentatives d'injection Datalog dans les noms de ressources
//! - Atténuations en chaîne multi-niveaux (transitivité du confinement)

use hakille_core::*;

fn future_ts(minutes: i64) -> Timestamp {
    Timestamp(chrono::Utc::now() + chrono::Duration::minutes(minutes))
}

fn past_ts(minutes: i64) -> Timestamp {
    Timestamp(chrono::Utc::now() - chrono::Duration::minutes(minutes))
}

// ============================================================================
// 1. CONFINEMENT STRICT ET ANTI-FUITE DES DONNÉES BRUTES
// ============================================================================

#[test]
fn test_adversarial_attenuated_token_cannot_leak_raw_sensors_or_biometrics() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();
    let exp = future_ts(30);

    // Token parent souverain avec droits complets sur capteurs, biométrie et inférences
    let parent = authority
        .create_root_token(
            vec![
                CapabilityScope::Camera,
                CapabilityScope::Audio,
                CapabilityScope::Biometrics,
                CapabilityScope::Spatial,
                CapabilityScope::CognitiveInference,
                CapabilityScope::Sensor,
            ],
            vec![CapabilityAction::Read, CapabilityAction::Write],
            exp.clone(),
        )
        .expect("Parent root token creation failed");

    // Vérification que le parent a bien accès aux données brutes
    assert!(authority.verify(&parent, CapabilityScope::Camera, CapabilityAction::Read).is_ok());
    assert!(authority.verify(&parent, CapabilityScope::Biometrics, CapabilityAction::Read).is_ok());
    assert!(authority.verify(&parent, CapabilityScope::Spatial, CapabilityAction::Read).is_ok());

    // Atténuation stricte pour un module tiers (UI ou widget) autorisant uniquement "emotion" et "presence"
    let attenuated = authority
        .attenuate_cognitive_inference(&parent, &["emotion", "presence"], future_ts(15))
        .expect("Attenuation should succeed");

    // A. L'accès aux ressources autorisées réussit
    assert!(authority.verify_cognitive_inference(&attenuated, "emotion").is_ok());
    assert!(authority.verify_cognitive_inference(&attenuated, "presence").is_ok());
    assert!(verify_cognitive_inference_with_root_key(&attenuated, &root_pub, "emotion").is_ok());

    // B. Tentatives d'accès aux flux bruts avec le token atténué : TOUTES DOIVENT ÉCHOUER (Fail-Closed)
    let raw_scopes = [
        CapabilityScope::Camera,
        CapabilityScope::Audio,
        CapabilityScope::Biometrics,
        CapabilityScope::Spatial,
        CapabilityScope::Sensor,
        CapabilityScope::FullSystem,
        CapabilityScope::Filesystem,
        CapabilityScope::Network,
    ];
    for scope in raw_scopes {
        let err = authority.verify(&attenuated, scope.clone(), CapabilityAction::Read);
        assert!(
            err.is_err(),
            "Adversarial breach: attenuated token should NEVER be granted raw scope {:?}",
            scope
        );
        let err_pinned = verify_capability_with_root_key(&attenuated, &root_pub, scope.clone(), CapabilityAction::Read);
        assert!(
            err_pinned.is_err(),
            "Adversarial breach: substrate pinned check should reject raw scope {:?}",
            scope
        );
    }

    // C. Tentative d'écriture (Write) au lieu de lecture (Read) sur cognitive_inference
    assert!(
        authority
            .verify(&attenuated, CapabilityScope::CognitiveInference, CapabilityAction::Write)
            .is_err(),
        "Adversarial breach: Write action should be forbidden by attenuation"
    );
}

// ============================================================================
// 2. ESCALADE HORIZONTALE ET RESSOURCES NON AUTORISÉES
// ============================================================================

#[test]
fn test_adversarial_unauthorized_cognitive_resources_rejected() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();
    let exp = future_ts(20);

    let parent = authority
        .create_root_token(
            vec![CapabilityScope::CognitiveInference],
            vec![CapabilityAction::Read],
            exp.clone(),
        )
        .unwrap();

    let attenuated = authority
        .attenuate_cognitive_inference(&parent, &["posture"], future_ts(10))
        .unwrap();

    // "posture" est autorisé
    assert!(verify_cognitive_inference_with_root_key(&attenuated, &root_pub, "posture").is_ok());

    // Tentatives d'accès à des ressources sensibles non autorisées
    let forbidden_resources = [
        "emotion",
        "estimated_age",
        "fatigue",
        "raw_landmarks",
        "camera_feed",
        "microphone_pcm",
        "all",
        "*",
        "posture_detailed",
    ];

    for forbidden in forbidden_resources {
        let res = verify_cognitive_inference_with_root_key(&attenuated, &root_pub, forbidden);
        assert!(
            res.is_err(),
            "Adversarial breach: unauthorized resource '{}' was accepted!",
            forbidden
        );
    }
}

// ============================================================================
// 3. FALSIFICATION CRYPTOGRAPHIQUE ET CLÉ RACINE DISSIDENTE
// ============================================================================

#[test]
fn test_adversarial_forged_root_key_and_tampered_bytes() {
    let genuine_authority = CapabilityAuthority::new();
    let genuine_root_pub = genuine_authority.root_public_key();

    // Attaquant dispose de sa propre autorité
    let rogue_authority = CapabilityAuthority::new();
    let rogue_token = rogue_authority
        .create_root_token(
            vec![CapabilityScope::CognitiveInference],
            vec![CapabilityAction::Read],
            future_ts(60),
        )
        .unwrap();

    // L'attaquant présente son token au substrat souverain épinglant la vraie clé
    let res = verify_cognitive_inference_with_root_key(&rogue_token, &genuine_root_pub, "emotion");
    assert!(
        res.is_err(),
        "Cryptographic forgery accepted! Token signed with rogue root key should fail verification"
    );

    // Altération de bytes dans un token légitime
    let genuine_parent = genuine_authority
        .create_root_token(
            vec![CapabilityScope::CognitiveInference],
            vec![CapabilityAction::Read],
            future_ts(60),
        )
        .unwrap();
    let genuine_attenuated = genuine_authority
        .attenuate_cognitive_inference(&genuine_parent, &["emotion"], future_ts(30))
        .unwrap();

    let mut token_bytes = genuine_attenuated.to_vec().unwrap();
    // Corruption d'un octet de signature
    let len = token_bytes.len();
    token_bytes[len - 5] ^= 0xFF;

    // Désérialisation avec la clé racine épinglée : doit échouer
    let deserialized = CapabilityToken::from_vec(token_bytes, &genuine_root_pub);
    assert!(
        deserialized.is_err(),
        "Tampered token bytes must fail deserialization and signature verification"
    );
}

// ============================================================================
// 4. VIOLATION DE MONOTONIE TEMPORELLE ET D'ESCALADE DE PRIVILÈGES
// ============================================================================

#[test]
fn test_adversarial_escalation_and_time_extension_rejected() {
    let authority = CapabilityAuthority::new();
    let parent_exp = future_ts(10);

    // Parent limité à CognitiveInference
    let parent = authority
        .create_root_token(
            vec![CapabilityScope::CognitiveInference],
            vec![CapabilityAction::Read],
            parent_exp.clone(),
        )
        .unwrap();

    // 1. Tentative d'atténuation en demandant un scope supérieur non détenu (Camera)
    let escalate_scope = authority.attenuate(
        &parent,
        vec![CapabilityScope::Camera],
        vec![CapabilityAction::Read],
        future_ts(5),
    );
    assert!(
        matches!(escalate_scope, Err(HakilleError::AttenuationViolation)),
        "Monotonicity violation: escalating to non-held scope must return AttenuationViolation"
    );

    // 2. Tentative d'extension temporelle (parent expire à +10 min, enfant demande +20 min)
    let extend_time = authority.attenuate_cognitive_inference(&parent, &["emotion"], future_ts(20));
    assert!(
        matches!(extend_time, Err(HakilleError::AttenuationViolation)),
        "Temporal monotonicity violation: child cannot outlive parent"
    );

    // 3. Token expiré dans le passé
    let expired_parent = authority
        .create_root_token(
            vec![CapabilityScope::CognitiveInference],
            vec![CapabilityAction::Read],
            past_ts(5),
        )
        .unwrap();
    let res = verify_cognitive_inference_with_root_key(
        &expired_parent,
        &authority.root_public_key(),
        "emotion",
    );
    assert!(
        matches!(res, Err(HakilleError::CapabilityExpired)),
        "Expired token must be rejected with CapabilityExpired"
    );
}

// ============================================================================
// 5. INJECTION DATALOG DANS LES NOMS DE RESSOURCES
// ============================================================================

#[test]
fn test_adversarial_datalog_injection_rejected() {
    let authority = CapabilityAuthority::new();
    let parent = authority
        .create_root_token(
            vec![CapabilityScope::CognitiveInference],
            vec![CapabilityAction::Read],
            future_ts(30),
        )
        .unwrap();

    let injection_payloads = [
        "emotion\"); check if true; //",
        "emotion\"\ncheck if true;",
        "test\\\"",
        "resource\r\nallowed_scope(\"camera\")",
        "emotion\", evil_fact(\"true",
        "", // Vide
    ];

    for payload in injection_payloads {
        let res = authority.attenuate_cognitive_inference(&parent, &[payload], future_ts(15));
        assert!(
            res.is_err(),
            "Datalog injection payload '{}' was not rejected by validation!",
            payload
        );
    }

    // Nom de ressource dépassant 256 caractères
    let long_payload = "a".repeat(257);
    assert!(
        authority
            .attenuate_cognitive_inference(&parent, &[&long_payload], future_ts(15))
            .is_err(),
        "Excessively long resource name must be rejected"
    );
}

// ============================================================================
// 6. ATTÉNUATION EN CHAÎNE MULTI-NIVEAUX (TRANSITIVITÉ)
// ============================================================================

#[test]
fn test_adversarial_chained_multi_level_attenuation() {
    let authority = CapabilityAuthority::new();
    let root_pub = authority.root_public_key();

    // Niveau 0 : Root (tous droits)
    let l0 = authority
        .create_root_token(
            vec![CapabilityScope::CognitiveInference, CapabilityScope::Spatial],
            vec![CapabilityAction::Read],
            future_ts(30),
        )
        .unwrap();

    // Niveau 1 : Atténué à ["emotion", "posture"]
    let l1 = authority
        .attenuate_cognitive_inference(&l0, &["emotion", "posture"], future_ts(20))
        .unwrap();
    assert!(verify_cognitive_inference_with_root_key(&l1, &root_pub, "emotion").is_ok());
    assert!(verify_cognitive_inference_with_root_key(&l1, &root_pub, "posture").is_ok());

    // Niveau 2 : Atténué davantage à ["emotion"] uniquement
    let l2 = authority
        .attenuate_cognitive_inference(&l1, &["emotion"], future_ts(10))
        .unwrap();
    assert!(verify_cognitive_inference_with_root_key(&l2, &root_pub, "emotion").is_ok());
    // "posture" ne doit PLUS être accessible par le petit-enfant
    assert!(verify_cognitive_inference_with_root_key(&l2, &root_pub, "posture").is_err());
    // "spatial" ne doit JAMAIS être accessible
    assert!(authority.verify(&l2, CapabilityScope::Spatial, CapabilityAction::Read).is_err());

    // Niveau 3 : Atténuation avec liste vide = Deny-all
    let l3 = authority
        .attenuate_cognitive_inference(&l2, &[], future_ts(5))
        .unwrap();
    assert!(verify_cognitive_inference_with_root_key(&l3, &root_pub, "emotion").is_err());
}
