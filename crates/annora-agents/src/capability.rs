use annora_core::{
    AnnoraError, CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken, Result,
};

pub struct AgentCapabilityVerifier {
    pub authority: CapabilityAuthority,
}

impl AgentCapabilityVerifier {
    pub fn new(authority: CapabilityAuthority) -> Self {
        Self { authority }
    }

    pub fn verify_action(
        &self,
        token: &CapabilityToken,
        scope: CapabilityScope,
        action: CapabilityAction,
    ) -> Result<()> {
        self.authority.verify(token, scope, action)
    }

    pub fn verify_attenuation(
        &self,
        _parent_token: &CapabilityToken,
        child_token: &CapabilityToken,
        child_scope: CapabilityScope,
        child_action: CapabilityAction,
    ) -> Result<()> {
        // Biscuit inherently verifies that any check added in blocks must hold
        self.authority.verify(child_token, child_scope, child_action)
            .map_err(|_| AnnoraError::AttenuationViolation)
    }
}
