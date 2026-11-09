//! Capability lifecycle state machine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 4-stage lifecycle state machine for managed capabilities.
///
/// Invariants:
/// - Registered -> Active or Disabled
/// - Active -> Disabled or Deprecated
/// - Disabled -> Active or Deprecated or [Unregistered]
/// - Deprecated -> Disabled or [Unregistered]
/// - An active capability cannot be unregistered directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CapabilityLifecycle {
    /// Manifest parsed and validated; not yet active for discovery or invocation.
    Registered { registered_at: DateTime<Utc> },
    /// Capability is enabled, actively indexed in discovery, and permitted for execution.
    Active {
        registered_at: DateTime<Utc>,
        activated_at: DateTime<Utc>,
    },
    /// Temporarily deactivated; preserved in storage but excluded from discovery and execution.
    Disabled {
        registered_at: DateTime<Utc>,
        disabled_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Remains functional for backwards compatibility but emits deprecation telemetry.
    Deprecated {
        registered_at: DateTime<Utc>,
        deprecated_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alternative_capability_id: Option<String>,
    },
}

impl CapabilityLifecycle {
    /// Initial lifecycle state upon registration.
    pub fn new_registered(registered_at: DateTime<Utc>) -> Self {
        Self::Registered { registered_at }
    }

    /// Whether this capability is currently active.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    /// Whether this capability is currently disabled.
    pub fn is_disabled(&self) -> bool {
        matches!(self, Self::Disabled { .. })
    }

    /// Whether this capability is currently deprecated.
    pub fn is_deprecated(&self) -> bool {
        matches!(self, Self::Deprecated { .. })
    }

    /// Whether this capability is in initial registered state.
    pub fn is_registered(&self) -> bool {
        matches!(self, Self::Registered { .. })
    }

    /// Returns the static name of the current lifecycle state.
    pub fn status_name(&self) -> &'static str {
        match self {
            Self::Registered { .. } => "registered",
            Self::Active { .. } => "active",
            Self::Disabled { .. } => "disabled",
            Self::Deprecated { .. } => "deprecated",
        }
    }

    /// Returns the original registration timestamp.
    pub fn registered_at(&self) -> DateTime<Utc> {
        match self {
            Self::Registered { registered_at }
            | Self::Active { registered_at, .. }
            | Self::Disabled { registered_at, .. }
            | Self::Deprecated { registered_at, .. } => *registered_at,
        }
    }

    /// Validates whether a transition to `Active` is permitted.
    pub fn can_transition_to_active(&self) -> bool {
        matches!(self, Self::Registered { .. } | Self::Disabled { .. })
    }

    /// Validates whether a transition to `Disabled` is permitted.
    pub fn can_transition_to_disabled(&self) -> bool {
        matches!(
            self,
            Self::Registered { .. } | Self::Active { .. } | Self::Deprecated { .. }
        )
    }

    /// Validates whether a transition to `Deprecated` is permitted.
    pub fn can_transition_to_deprecated(&self) -> bool {
        matches!(self, Self::Active { .. } | Self::Disabled { .. })
    }

    /// Validates whether this capability can be removed from the registry.
    pub fn can_unregister(&self) -> bool {
        !self.is_active()
    }
}
