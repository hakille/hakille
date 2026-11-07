//! Universal Capability Architecture for Hakille.
//!
//! Provides the canonical contract for tools, capability manifests, descriptors,
//! execution telemetry, 4-tier risk classification, lossless bidirectional
//! translators across OpenAI, Anthropic, Gemini, and ChatML formats,
//! universal capability registry, hybrid semantic discovery, execution engine,
//! permission & security engine, and cryptographic audit adapter.

pub mod adapters;
pub mod audit;
pub mod discovery;
pub mod error;
pub mod execution;
pub mod registry;
pub mod security;
pub mod translators;
pub mod types;
pub mod validation;

pub use adapters::*;
pub use audit::CapabilityAuditAdapter;
pub use discovery::{
    CapabilityDiscoveryEngine, DiscoveredTool, DiscoveryError, DiscoveryQuery, cosine_similarity,
    estimate_tool_tokens, infer_tool_risk,
};
pub use error::{CapabilityError, Result, ValidationError};
pub use execution::{
    CapabilityExecutor, ExecutionContext, StructuredToolError, UniversalExecutionEngine,
};
pub use registry::{
    CapabilityLifecycle, RegistryEntry, RegistryError, RegistryInner, UniversalCapabilityRegistry,
    resolve_version,
};
pub use security::{
    ActionConfirmationProof, AntiSsrfValidator, BiscuitVerifier, RiskVerificationError, SecretRef,
    SecretVault, SecretVaultError, SsrfError,
};
pub use translators::{ProviderTranslator, SchemaNormalizer, TranslatorError};
pub use types::*;
pub use validation::*;
