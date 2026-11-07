//! Hakille E2E Requirement-Driven Test Suite.
//!
//! Covers all 4 testing tiers across all 20 features:
//! - Tier 1: Feature Coverage (100 tests, `T1_F01_01` to `T1_F20_05`)
//! - Tier 2: Boundary & Corner Cases (100 tests, `T2_F01_01` to `T2_F20_05`)
//! - Tier 3: Combinatorial Cross-Feature Pairs (20 tests, `T3_PAIR_01` to `T3_PAIR_20`)
//! - Tier 4: Real-World Scenarios (10 tests, `T4_SCENARIO_01` to `T4_SCENARIO_10`)

pub mod harness;

#[cfg(test)]
pub mod tier1_features;
#[cfg(test)]
pub mod tier2_boundary;
#[cfg(test)]
pub mod tier3_combinatorial;
#[cfg(test)]
pub mod tier4_scenarios;
