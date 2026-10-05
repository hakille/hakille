use annora_core::{
    CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp,
};

#[test]
fn test_capability_creation_and_verification() {
    let authority = CapabilityAuthority::new();
    let future_expiry = Timestamp(chrono::Utc::now() + chrono::Duration::hours(2));

    let root_token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem, CapabilityScope::Network],
            vec![CapabilityAction::Read, CapabilityAction::Write],
            future_expiry.clone(),
        )
        .expect("Root token creation should succeed");

    // Verification of granted rights should succeed
    authority
        .verify(&root_token, CapabilityScope::Filesystem, CapabilityAction::Read)
        .expect("Filesystem Read should be verified");

    // Verification of ungranted rights should fail
    let ungranted = authority.verify(&root_token, CapabilityScope::GuiInput, CapabilityAction::Read);
    assert!(ungranted.is_err());
}

#[test]
fn test_offline_attenuation() {
    let authority = CapabilityAuthority::new();
    let future_expiry = Timestamp(chrono::Utc::now() + chrono::Duration::hours(2));

    let root_token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem, CapabilityScope::Network],
            vec![CapabilityAction::Read, CapabilityAction::Write],
            future_expiry.clone(),
        )
        .unwrap();

    // Attenuate to only Filesystem Read
    let attenuated = authority
        .attenuate(
            &root_token,
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future_expiry,
        )
        .expect("Attenuation should succeed");

    // Filesystem Read should still be allowed
    authority
        .verify(&attenuated, CapabilityScope::Filesystem, CapabilityAction::Read)
        .expect("Filesystem Read should pass on attenuated token");
}
