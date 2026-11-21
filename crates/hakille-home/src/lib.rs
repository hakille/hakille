//! # Hakille Home — Domotique Souveraine Multi-Protocoles, Jumeau Spatial 3D & Homéostasie Énergétique
//!
//! Zéro-cloud par défaut, épinglage cryptographique Biscuit v5, garde réseau local strict
//! (`HomeLanGuard`), maintien physique 600 ms pour les actions critiques Tier 3, et aucun
//! succès simulé.

pub mod automation;
pub mod digital_twin;
pub mod energy;
pub mod hub;
pub mod intent;
pub mod mesh_sync;
pub mod model;
pub mod protocols;
pub mod security;

pub use automation::{
    AutomationAction, AutomationCondition, AutomationEngine, AutomationEscalation,
    AutomationEvaluationReport, AutomationRule, AutomationTrigger, PlannedAutoDispatch,
};
pub use digital_twin::{HomeDigitalTwin, SpatialDeviceHit, SpatialIntent};
pub use energy::{
    AnomalySeverity, DailyEnergyReport, EnergyAnomaly, EnergyArbitrageAction, EnergyBalance,
    EnergyDialecticReport, EnergyHomeostasisEngine, EnergySample, TariffPlan, TempoDayColor,
    compute_daily_kwh_and_cost,
};
pub use hub::{HomeDryRunPreview, HomeScene, SovereignHomeHub};
pub use intent::{ResolvedHomeIntent, resolve_home_intent};
pub use mesh_sync::{
    HomeMeshSyncReport, HomeTwinSyncDelta, broadcast_home_delta_on_mesh, build_sync_delta,
    merge_devices_lww, validate_mesh_peer_endpoint,
};
pub use model::{
    CommandReceipt, ConnectionStatus, DeviceCategory, DeviceState, DiscoveryRecord,
    EnergyTelemetry, HomeCommand, HomeDevice, HomeError, HomeEvent, HomeResult, HomeRiskTier,
    ProtocolCapabilityInfo, ProtocolKind, RoomZone, SpatialPose3D,
};
pub use security::{
    HomeBiscuitPolicy, HomeLanGuard, HomeResidentRole, HomeRiskClassifier, PhysicalHoldProof,
    new_ephemeral_authority,
};
