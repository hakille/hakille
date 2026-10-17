use annora_core::{CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp};
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_attenuation_cannot_escalate_scopes(
        has_fs in any::<bool>(),
        has_net in any::<bool>(),
        has_wallet in any::<bool>(),
    ) {
        let authority = CapabilityAuthority::new();
        let mut initial_scopes = Vec::new();
        if has_fs { initial_scopes.push(CapabilityScope::Filesystem); }
        if has_net { initial_scopes.push(CapabilityScope::Network); }
        if has_wallet { initial_scopes.push(CapabilityScope::Wallet); }

        // Root token created with the initial scopes
        let root = authority.create_root_token(
            initial_scopes.clone(),
            vec![CapabilityAction::Read],
            Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
        ).unwrap();

        // Any scope in initial_scopes must verify
        for scope in &initial_scopes {
            prop_assert!(authority.verify(&root, scope.clone(), CapabilityAction::Read).is_ok());
        }

        // An unauthorized scope must NEVER verify on the root token
        if !has_fs {
            prop_assert!(authority.verify(&root, CapabilityScope::Filesystem, CapabilityAction::Read).is_err());
        }
        if !has_net {
            prop_assert!(authority.verify(&root, CapabilityScope::Network, CapabilityAction::Read).is_err());
        }
        if !has_wallet {
            prop_assert!(authority.verify(&root, CapabilityScope::Wallet, CapabilityAction::Read).is_err());
        }

        // If attenuated with an empty scope, none of the operations should succeed
        let attenuated = authority.attenuate(
            &root,
            vec![],
            vec![],
            Timestamp(chrono::Utc::now() + chrono::Duration::minutes(5)),
        ).unwrap();

        prop_assert!(authority.verify(&attenuated, CapabilityScope::Filesystem, CapabilityAction::Read).is_err());
        prop_assert!(authority.verify(&attenuated, CapabilityScope::Network, CapabilityAction::Read).is_err());
        prop_assert!(authority.verify(&attenuated, CapabilityScope::Wallet, CapabilityAction::Read).is_err());
    }
}
