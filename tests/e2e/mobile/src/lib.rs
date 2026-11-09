//! Hakille Mobile Opaque-Box E2E Test Suite.
//!
//! Requirement-driven tests for all 30 mobile features across 4 tiers:
//! - Tier 1: Feature Coverage in Isolation (150 tests)
//! - Tier 2: Boundary Values & Fault Injection (150 tests)
//! - Tier 3: Pairwise Combinatorial Interactions (30 tests)
//! - Tier 4: Real-World Workload Scenarios (6 scenarios)

pub mod harness;
#[cfg_attr(not(test), allow(unused_imports, dead_code))]
pub mod tier1_features;
#[cfg_attr(not(test), allow(unused_imports, dead_code))]
pub mod tier2_boundaries;
#[cfg_attr(not(test), allow(unused_imports, dead_code))]
pub mod tier3_pairwise;
#[cfg_attr(not(test), allow(unused_imports, dead_code))]
pub mod tier4_workloads;

pub use harness::*;
