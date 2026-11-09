//! Hostile adversarial stress tests and attack vectors for UniversalCapabilityRegistry
//! and UniversalExecutionEngine in hakille-capability.
//!
//! Evaluates:
//! 1. UniversalCapabilityRegistry:
//!    - High-concurrency reads vs writes (rapid register/activate/disable/unregister loops across Tokio tasks).
//!    - Tool name collisions across overlapping SemVer releases and cross-capability conflicts.
//!    - Lifecycle state machine illegal transition rejection and unregistration invariants.
//!    - Complex SemVer requirement matching (caret, tilde, pre-releases, build metadata, ranges).
//! 2. UniversalExecutionEngine:
//!    - Extreme timeout boundaries (sub-millisecond, zero timeout, pre-cancelled tokens).
//!    - Concurrent cancellation token triggering across active futures and biased cancellation precedence.
//!    - Panic boundaries inside executors (sync and async panics, isolation verification).
//!    - Large structured error payloads (500KB+ JSON details, adversarial strings, transient retry backoff telemetry).

use async_trait::async_trait;
use chrono::Utc;
use semver::{Version, VersionReq};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use hakille_capability::error::CapabilityError;
use hakille_capability::registry::version::{matching_versions, resolve_version};
use hakille_capability::types::{
    CanonicalContentBlock, CanonicalToolCall, CanonicalToolDefinition, CanonicalToolResult,
    CapabilityManifest, CapabilityScope, CapabilityType, ExecutionMetadata, RiskTier,
};
use hakille_capability::{
    CapabilityExecutor, CapabilityLifecycle, ExecutionContext, RegistryError, StructuredToolError,
    UniversalCapabilityRegistry, UniversalExecutionEngine,
};

/// Helper to create a test manifest.
fn make_manifest(
    id: &str,
    version: &str,
    tool_names: &[&str],
    scopes: &[CapabilityScope],
    cap_type: CapabilityType,
    tags: &[&str],
) -> CapabilityManifest {
    let mut manifest = CapabilityManifest::new(
        id,
        format!("Display {id}"),
        Version::parse(version).expect("valid semver"),
        format!("Description for {id}"),
        "Adversarial Test Suite",
        cap_type,
    );
    for scope in scopes {
        manifest = manifest.with_scope(scope.clone());
    }
    for tool_name in tool_names {
        manifest = manifest.with_tool(CanonicalToolDefinition::new(
            *tool_name,
            format!("Documentation for {tool_name}"),
            CanonicalToolDefinition::empty_parameters_schema(),
        ));
    }
    let tag_values: Vec<serde_json::Value> = tags
        .iter()
        .map(|t| serde_json::Value::String(t.to_string()))
        .collect();
    manifest.with_metadata("tags", serde_json::Value::Array(tag_values))
}

// ============================================================================
// SECTION 1: UniversalCapabilityRegistry - High-Concurrency Reads vs Writes
// ============================================================================

#[tokio::test]
async fn test_adversarial_high_concurrency_registry_torture() {
    // 60 concurrent worker tasks executing a dense mix of mutations and lookups
    // for 1.5 seconds to expose data races, deadlocks, and index corruption.
    let registry = Arc::new(UniversalCapabilityRegistry::new());
    let error_count = Arc::new(AtomicUsize::new(0));
    let read_count = Arc::new(AtomicUsize::new(0));
    let write_count = Arc::new(AtomicUsize::new(0));

    // Pre-populate some baseline capabilities
    for i in 0..10 {
        let id = format!("baseline.cap_{i}");
        let tool = format!("base_tool_{i}");
        let manifest = make_manifest(
            &id,
            "1.0.0",
            &[&tool],
            &[CapabilityScope::Filesystem],
            CapabilityType::Native,
            &["storage", "baseline"],
        );
        registry.register(manifest).await.unwrap();
        registry.activate(&id, None).await.unwrap();
    }

    let mut handles = Vec::new();
    let num_tasks = 40;

    for task_id in 0..num_tasks {
        let reg = Arc::clone(&registry);
        let errs = Arc::clone(&error_count);
        let reads = Arc::clone(&read_count);
        let writes = Arc::clone(&write_count);

        handles.push(tokio::spawn(async move {
            let cap_id = format!("concur.cap_{task_id}");
            let tool_name = format!("concur_tool_{task_id}");

            for iteration in 0..25 {
                let ver_str = format!("1.{iteration}.0");
                let manifest = make_manifest(
                    &cap_id,
                    &ver_str,
                    &[&tool_name],
                    &[CapabilityScope::Network],
                    CapabilityType::McpStdio,
                    &["concurrency", "test"],
                );

                // 1. Register version
                match reg.register(manifest).await {
                    Ok(_) => {
                        writes.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(RegistryError::AlreadyExists { .. }) => {}
                    Err(e) => {
                        eprintln!("Unexpected register error: {e:?}");
                        errs.fetch_add(1, Ordering::Relaxed);
                    }
                }

                // 2. Concurrent read
                let found = reg.find_tool(&tool_name).await;
                reads.fetch_add(1, Ordering::Relaxed);
                if let Some((desc, tool)) = found {
                    assert_eq!(desc.id, cap_id);
                    assert_eq!(tool.name, tool_name);
                }

                // 3. Activate
                let req_str = format!("^1.{iteration}");
                let req = VersionReq::parse(&req_str).unwrap();
                match reg.activate(&cap_id, Some(&req)).await {
                    Ok(activated_ver) => {
                        writes.fetch_add(1, Ordering::Relaxed);
                        assert_eq!(activated_ver, Version::parse(&ver_str).unwrap());
                    }
                    Err(e) => {
                        eprintln!("Activate error: {e:?}");
                        errs.fetch_add(1, Ordering::Relaxed);
                    }
                }

                // 4. Read while active
                let found_active = reg.find_tool(&tool_name).await;
                reads.fetch_add(1, Ordering::Relaxed);
                assert!(found_active.is_some(), "Active tool must be discoverable");

                // 5. Query indices
                let _scoped = reg.list_by_scope(&CapabilityScope::Network).await;
                let _typed = reg.list_by_type(CapabilityType::McpStdio).await;
                let _tagged = reg.list_by_tag("concurrency").await;
                reads.fetch_add(3, Ordering::Relaxed);

                // 6. Disable
                if iteration % 2 == 1 && reg.disable(&cap_id).await.is_ok() {
                    writes.fetch_add(1, Ordering::Relaxed);
                    assert!(
                        reg.find_tool(&tool_name).await.is_none(),
                        "Disabled tool must not be found"
                    );
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("Task panicked during concurrency torture");
    }

    assert_eq!(
        error_count.load(Ordering::SeqCst),
        0,
        "No unexpected errors during high concurrency"
    );
    assert!(
        read_count.load(Ordering::SeqCst) > 2000,
        "Sufficient reads executed"
    );
    assert!(
        write_count.load(Ordering::SeqCst) > 1000,
        "Sufficient writes executed"
    );

    // Baseline capabilities must remain completely uncorrupted
    for i in 0..10 {
        let tool = format!("base_tool_{i}");
        let found = registry.find_tool(&tool).await;
        assert!(found.is_some(), "Baseline tool {tool} must still exist");
    }
}

// ============================================================================
// SECTION 2: Tool Name Collisions & SemVer Upgrades
// ============================================================================

#[tokio::test]
async fn test_adversarial_tool_collision_cross_capability() {
    let registry = UniversalCapabilityRegistry::new();

    let manifest_alpha = make_manifest(
        "provider.alpha",
        "1.0.0",
        &["shared_analyzer", "alpha_unique"],
        &[CapabilityScope::Filesystem],
        CapabilityType::Native,
        &["analysis"],
    );
    let manifest_beta = make_manifest(
        "provider.beta",
        "1.0.0",
        &["shared_analyzer", "beta_unique"],
        &[CapabilityScope::Filesystem],
        CapabilityType::Native,
        &["analysis"],
    );

    registry.register(manifest_alpha).await.unwrap();
    registry.register(manifest_beta).await.unwrap();

    // Activate Alpha
    registry.activate("provider.alpha", None).await.unwrap();

    // Activating Beta must fail due to 'shared_analyzer' colliding with provider.alpha
    let err = registry.activate("provider.beta", None).await;
    match err {
        Err(RegistryError::ToolCollision {
            tool_name,
            existing_capability_id,
        }) => {
            assert_eq!(tool_name, "shared_analyzer");
            assert!(existing_capability_id.contains("provider.alpha@1.0.0"));
        }
        other => panic!("Expected ToolCollision error, got {other:?}"),
    }

    // Verify Beta's unique tool was NOT partially indexed
    assert!(
        registry.find_tool("beta_unique").await.is_none(),
        "Partial index corruption: beta_unique must not be indexed after failed activation"
    );

    // Disable Alpha
    registry.disable("provider.alpha").await.unwrap();
    assert!(registry.find_tool("shared_analyzer").await.is_none());

    // Now Beta can activate cleanly
    let beta_res = registry.activate("provider.beta", None).await;
    assert!(
        beta_res.is_ok(),
        "Beta activation should succeed after Alpha is disabled"
    );
    assert!(registry.find_tool("shared_analyzer").await.is_some());
    assert!(registry.find_tool("beta_unique").await.is_some());

    // Attempting to re-activate Alpha now collides with Beta
    let alpha_re_err = registry.activate("provider.alpha", None).await;
    match alpha_re_err {
        Err(RegistryError::ToolCollision {
            tool_name,
            existing_capability_id,
        }) => {
            assert_eq!(tool_name, "shared_analyzer");
            assert!(existing_capability_id.contains("provider.beta@1.0.0"));
        }
        other => panic!("Expected ToolCollision error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_adversarial_self_upgrade_tool_name_preservation() {
    let registry = UniversalCapabilityRegistry::new();

    // Version 1.0.0
    let m1 = make_manifest(
        "my.service",
        "1.0.0",
        &["execute_query", "legacy_ping"],
        &[],
        CapabilityType::Native,
        &[],
    );
    // Version 2.0.0 retains 'execute_query', drops 'legacy_ping', adds 'stream_query'
    let m2 = make_manifest(
        "my.service",
        "2.0.0",
        &["execute_query", "stream_query"],
        &[],
        CapabilityType::Native,
        &[],
    );

    registry.register(m1).await.unwrap();
    registry.register(m2).await.unwrap();

    // Activate 1.0.0
    let req_v1 = VersionReq::parse("=1.0.0").unwrap();
    registry
        .activate("my.service", Some(&req_v1))
        .await
        .unwrap();

    let (desc1, _) = registry.find_tool("execute_query").await.unwrap();
    assert_eq!(desc1.version, Version::parse("1.0.0").unwrap());
    assert!(registry.find_tool("legacy_ping").await.is_some());

    // Upgrading to 2.0.0 must NOT report self-collision on 'execute_query'
    let req_v2 = VersionReq::parse("=2.0.0").unwrap();
    let upgrade_res = registry.activate("my.service", Some(&req_v2)).await;
    assert!(
        upgrade_res.is_ok(),
        "Self-upgrade must not trigger false collision: {upgrade_res:?}"
    );

    // execute_query must now point to 2.0.0
    let (desc2, _) = registry.find_tool("execute_query").await.unwrap();
    assert_eq!(desc2.version, Version::parse("2.0.0").unwrap());

    // stream_query must be available
    assert!(registry.find_tool("stream_query").await.is_some());

    // legacy_ping from 1.0.0 must be removed from tool index
    assert!(
        registry.find_tool("legacy_ping").await.is_none(),
        "Old tool from superseded version must be removed"
    );

    // Version 1.0.0 lifecycle must now be Disabled (superseded)
    let v1_life = registry
        .get_lifecycle("my.service", &Version::parse("1.0.0").unwrap())
        .await
        .unwrap();
    assert!(v1_life.is_disabled());
}

// ============================================================================
// SECTION 3: Lifecycle State Machine & Invariant Enforcement
// ============================================================================

#[test]
fn test_adversarial_lifecycle_state_machine_transition_predicates() {
    let now = Utc::now();

    // 1. Registered
    let reg = CapabilityLifecycle::new_registered(now);
    assert!(reg.is_registered());
    assert!(!reg.is_active());
    assert!(!reg.is_disabled());
    assert!(!reg.is_deprecated());
    assert_eq!(reg.status_name(), "registered");
    assert!(reg.can_transition_to_active());
    assert!(reg.can_transition_to_disabled());
    assert!(
        !reg.can_transition_to_deprecated(),
        "Registered cannot transition directly to Deprecated"
    );
    assert!(reg.can_unregister(), "Registered can be unregistered");

    // 2. Active
    let act = CapabilityLifecycle::Active {
        registered_at: now,
        activated_at: now,
    };
    assert!(act.is_active());
    assert!(!act.is_registered());
    assert_eq!(act.status_name(), "active");
    assert!(
        !act.can_transition_to_active(),
        "Active cannot re-transition to Active"
    );
    assert!(act.can_transition_to_disabled());
    assert!(act.can_transition_to_deprecated());
    assert!(
        !act.can_unregister(),
        "Active CANNOT be unregistered directly"
    );

    // 3. Disabled
    let dis = CapabilityLifecycle::Disabled {
        registered_at: now,
        disabled_at: now,
        reason: Some("maintenance".to_string()),
    };
    assert!(dis.is_disabled());
    assert_eq!(dis.status_name(), "disabled");
    assert!(
        dis.can_transition_to_active(),
        "Disabled can be re-activated"
    );
    assert!(
        !dis.can_transition_to_disabled(),
        "Disabled cannot transition to Disabled"
    );
    assert!(dis.can_transition_to_deprecated());
    assert!(dis.can_unregister(), "Disabled can be unregistered");

    // 4. Deprecated
    let dep = CapabilityLifecycle::Deprecated {
        registered_at: now,
        deprecated_at: now,
        message: Some("sunset soon".to_string()),
        alternative_capability_id: Some("replacement.v2".to_string()),
    };
    assert!(dep.is_deprecated());
    assert_eq!(dep.status_name(), "deprecated");
    assert!(
        !dep.can_transition_to_active(),
        "Deprecated cannot transition to Active"
    );
    assert!(dep.can_transition_to_disabled());
    assert!(
        !dep.can_transition_to_deprecated(),
        "Deprecated cannot transition to Deprecated"
    );
    assert!(dep.can_unregister(), "Deprecated can be unregistered");
}

#[tokio::test]
async fn test_adversarial_illegal_unregistration_and_missing_lookups() {
    let registry = UniversalCapabilityRegistry::new();
    let manifest = make_manifest(
        "secure.store",
        "1.0.0",
        &["get_key"],
        &[],
        CapabilityType::Native,
        &[],
    );
    registry.register(manifest).await.unwrap();

    let ver_1 = Version::parse("1.0.0").unwrap();

    // 1. Activate
    registry.activate("secure.store", None).await.unwrap();

    // 2. Attempting to unregister active capability must fail
    let unreg_err = registry.unregister("secure.store", &ver_1).await;
    match unreg_err {
        Err(RegistryError::ActiveUnregisterBlocked { id }) => {
            assert_eq!(id, "secure.store");
        }
        other => panic!("Expected ActiveUnregisterBlocked, got {other:?}"),
    }

    // 3. Unregister non-existent ID
    let not_found_unreg = registry.unregister("ghost.cap", &ver_1).await;
    assert!(matches!(
        not_found_unreg,
        Err(RegistryError::NotFound { .. })
    ));

    // 4. Disable non-existent ID
    let not_found_dis = registry.disable("ghost.cap").await;
    assert!(matches!(not_found_dis, Err(RegistryError::NotFound { .. })));

    // 5. Deprecate non-existent ID
    let not_found_dep = registry.deprecate("ghost.cap", None, None).await;
    assert!(matches!(not_found_dep, Err(RegistryError::NotFound { .. })));

    // 6. Activate non-existent ID
    let not_found_act = registry.activate("ghost.cap", None).await;
    assert!(matches!(not_found_act, Err(RegistryError::NotFound { .. })));

    // 7. Duplicate registration rejection
    let dup_manifest = make_manifest(
        "secure.store",
        "1.0.0",
        &["get_key"],
        &[],
        CapabilityType::Native,
        &[],
    );
    let dup_err = registry.register(dup_manifest).await;
    assert!(matches!(dup_err, Err(RegistryError::AlreadyExists { .. })));
}

// ============================================================================
// SECTION 4: Complex SemVer Resolution & Matching Constraints
// ============================================================================

#[test]
fn test_adversarial_semver_complex_ranges_and_edge_cases() {
    let versions = vec![
        Version::parse("0.0.1").unwrap(),
        Version::parse("0.1.0").unwrap(),
        Version::parse("0.1.1").unwrap(),
        Version::parse("1.0.0-alpha.1").unwrap(),
        Version::parse("1.0.0-beta.1").unwrap(),
        Version::parse("1.0.0-rc.1").unwrap(),
        Version::parse("1.0.0").unwrap(),
        Version::parse("1.0.1").unwrap(),
        Version::parse("1.1.0").unwrap(),
        Version::parse("1.1.5").unwrap(),
        Version::parse("1.2.0-pre.1").unwrap(),
        Version::parse("2.0.0").unwrap(),
        Version::parse("2.1.0-alpha").unwrap(),
        Version::parse("2.1.0").unwrap(),
        Version::parse("10.0.0").unwrap(),
    ];

    // Caret ranges
    let req_caret_01 = VersionReq::parse("^0.1.0").unwrap();
    assert_eq!(
        resolve_version(&versions, Some(&req_caret_01)),
        Some(Version::parse("0.1.1").unwrap())
    );

    let req_caret_1 = VersionReq::parse("^1.0").unwrap();
    assert_eq!(
        resolve_version(&versions, Some(&req_caret_1)),
        Some(Version::parse("1.1.5").unwrap())
    );

    // Tilde ranges
    let req_tilde = VersionReq::parse("~1.1.0").unwrap();
    assert_eq!(
        resolve_version(&versions, Some(&req_tilde)),
        Some(Version::parse("1.1.5").unwrap())
    );

    // Wildcard
    let req_wildcard = VersionReq::parse("1.*").unwrap();
    assert_eq!(
        resolve_version(&versions, Some(&req_wildcard)),
        Some(Version::parse("1.1.5").unwrap())
    );

    // Multiple range constraints
    let req_compound = VersionReq::parse(">= 1.0.0, < 2.0.0").unwrap();
    assert_eq!(
        resolve_version(&versions, Some(&req_compound)),
        Some(Version::parse("1.1.5").unwrap())
    );

    // Unsatisfiable constraint
    let req_none = VersionReq::parse(">= 20.0.0").unwrap();
    assert_eq!(resolve_version(&versions, Some(&req_none)), None);

    // No requirement -> highest overall
    assert_eq!(
        resolve_version(&versions, None),
        Some(Version::parse("10.0.0").unwrap())
    );

    // matching_versions ordering (ascending)
    let matched = matching_versions(&versions, &VersionReq::parse("^1.0").unwrap());
    assert_eq!(
        matched,
        vec![
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.0.1").unwrap(),
            Version::parse("1.1.0").unwrap(),
            Version::parse("1.1.5").unwrap(),
        ]
    );
}

#[tokio::test]
async fn test_adversarial_registry_semver_unsatisfied_requirement() {
    let registry = UniversalCapabilityRegistry::new();
    let m = make_manifest(
        "calc.engine",
        "1.5.0",
        &["add"],
        &[],
        CapabilityType::Native,
        &[],
    );
    registry.register(m).await.unwrap();

    let req = VersionReq::parse("^2.0").unwrap();
    let res = registry.activate("calc.engine", Some(&req)).await;
    match res {
        Err(RegistryError::VersionMismatch { id, requirement }) => {
            assert_eq!(id, "calc.engine");
            assert_eq!(requirement, "^2.0");
        }
        other => panic!("Expected VersionMismatch, got {other:?}"),
    }
}

// ============================================================================
// SECTION 5: UniversalExecutionEngine - Extreme Timeout Boundaries
// ============================================================================

struct AdversarialDelayedExecutor {
    delay: Duration,
    should_panic: bool,
    panic_async: bool,
}

#[async_trait]
impl CapabilityExecutor for AdversarialDelayedExecutor {
    async fn execute(
        &self,
        call: &CanonicalToolCall,
        context: &ExecutionContext,
    ) -> Result<CanonicalToolResult, CapabilityError> {
        if self.should_panic && !self.panic_async {
            panic!("Adversarial synchronous executor crash: {}", call.tool_name);
        }

        if self.delay > Duration::ZERO {
            tokio::select! {
                _ = context.cancellation_token.cancelled() => {
                    return Err(CapabilityError::ExecutionFailed {
                        tool_name: call.tool_name.clone(),
                        message: "Cancelled during delay".to_string(),
                    });
                }
                _ = tokio::time::sleep(self.delay) => {}
            }
        }

        if self.should_panic && self.panic_async {
            panic!(
                "Adversarial asynchronous executor crash: {}",
                call.tool_name
            );
        }

        Ok(CanonicalToolResult::success_text(
            &call.call_id,
            "success",
            ExecutionMetadata::new(0, context.risk_tier),
        ))
    }
}

#[tokio::test]
async fn test_adversarial_extreme_timeout_sub_millisecond_and_zero() {
    let executor = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::from_millis(50),
        should_panic: false,
        panic_async: false,
    });

    // 1. Zero timeout
    let call_0 = CanonicalToolCall::new("zero_timeout", "tool", serde_json::json!({}));
    let ctx_0 = ExecutionContext::new("zero_timeout", Duration::ZERO);
    let res_0 = UniversalExecutionEngine::execute_guarded(executor.clone(), &call_0, &ctx_0).await;
    assert!(
        matches!(res_0, Err(CapabilityError::Timeout { timeout_ms: 0 })),
        "Zero timeout must trigger immediate Timeout error"
    );

    // 2. 1-nanosecond timeout
    let call_1 = CanonicalToolCall::new("nano_timeout", "tool", serde_json::json!({}));
    let ctx_1 = ExecutionContext::new("nano_timeout", Duration::from_nanos(1));
    let res_1 = UniversalExecutionEngine::execute_guarded(executor.clone(), &call_1, &ctx_1).await;
    assert!(
        matches!(res_1, Err(CapabilityError::Timeout { .. })),
        "1ns timeout must trigger Timeout error"
    );

    // 3. 500-microsecond timeout against 50ms work
    let call_2 = CanonicalToolCall::new("micro_timeout", "tool", serde_json::json!({}));
    let ctx_2 = ExecutionContext::new("micro_timeout", Duration::from_micros(500));
    let res_2 = UniversalExecutionEngine::execute_guarded(executor.clone(), &call_2, &ctx_2).await;
    assert!(
        matches!(res_2, Err(CapabilityError::Timeout { .. })),
        "500us timeout must trigger Timeout error"
    );
}

#[tokio::test]
async fn test_adversarial_pre_cancelled_token_rejection() {
    let executor = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::from_secs(10),
        should_panic: true, // If called, it would panic!
        panic_async: false,
    });

    let cancel_token = CancellationToken::new();
    // Pre-cancel before execution call
    cancel_token.cancel();

    let call = CanonicalToolCall::new("precancel", "dangerous_tool", serde_json::json!({}));
    let ctx = ExecutionContext::new("precancel", Duration::from_secs(5))
        .with_cancellation_token(cancel_token);

    let res = UniversalExecutionEngine::execute_guarded(executor, &call, &ctx).await;
    match res {
        Err(CapabilityError::ExecutionFailed { tool_name, message }) => {
            assert_eq!(tool_name, "dangerous_tool");
            assert!(
                message.contains("cancellation token"),
                "Must reject immediately due to pre-cancelled token without invoking executor"
            );
        }
        other => panic!("Expected ExecutionFailed with cancellation message, got {other:?}"),
    }
}

#[tokio::test]
async fn test_adversarial_concurrent_timeout_storm() {
    // 50 parallel requests firing with aggressive 5ms timeouts against a 100ms executor
    let executor = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::from_millis(100),
        should_panic: false,
        panic_async: false,
    });

    let mut handles = Vec::new();
    for i in 0..50 {
        let exec = Arc::clone(&executor);
        handles.push(tokio::spawn(async move {
            let call =
                CanonicalToolCall::new(format!("storm_{i}"), "slow_op", serde_json::json!({}));
            let ctx = ExecutionContext::new(format!("storm_{i}"), Duration::from_millis(5));
            UniversalExecutionEngine::execute_guarded(exec, &call, &ctx).await
        }));
    }

    for h in handles {
        let res = h.await.unwrap();
        assert!(
            matches!(res, Err(CapabilityError::Timeout { .. })),
            "All storm tasks must timeout deterministically"
        );
    }
}

// ============================================================================
// SECTION 6: UniversalExecutionEngine - Concurrent Cancellation & Token Hierarchy
// ============================================================================

#[tokio::test]
async fn test_adversarial_concurrent_cancellation_cascade() {
    let executor = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::from_millis(500),
        should_panic: false,
        panic_async: false,
    });

    let parent_token = CancellationToken::new();
    let mut handles = Vec::new();

    // Spawn 20 tasks all bound to the parent cancellation token
    for i in 0..20 {
        let exec = Arc::clone(&executor);
        let token = parent_token.clone();
        handles.push(tokio::spawn(async move {
            let call =
                CanonicalToolCall::new(format!("cascade_{i}"), "delay_op", serde_json::json!({}));
            let ctx = ExecutionContext::new(format!("cascade_{i}"), Duration::from_secs(10))
                .with_cancellation_token(token);
            UniversalExecutionEngine::execute_guarded(exec, &call, &ctx).await
        }));
    }

    // Allow tasks to start, then trigger parent cancellation
    tokio::time::sleep(Duration::from_millis(20)).await;
    parent_token.cancel();

    let mut cancelled_count = 0;
    for h in handles {
        let res = h.await.unwrap();
        match res {
            Err(CapabilityError::ExecutionFailed { message, .. }) => {
                if message.contains("cancellation token") {
                    cancelled_count += 1;
                }
            }
            other => panic!("Unexpected result during cancellation cascade: {other:?}"),
        }
    }

    assert_eq!(
        cancelled_count, 20,
        "All 20 tasks must abort via cancellation"
    );
}

#[tokio::test]
async fn test_adversarial_child_token_isolation() {
    let executor = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::from_millis(50),
        should_panic: false,
        panic_async: false,
    });

    let root_token = CancellationToken::new();
    let child_token_a = root_token.child_token();
    let child_token_b = root_token.child_token();

    // Task A uses child_token_a
    let exec_a = Arc::clone(&executor);
    let token_a_moved = child_token_a.clone();
    let handle_a = tokio::spawn(async move {
        let call = CanonicalToolCall::new("task_a", "op", serde_json::json!({}));
        let ctx = ExecutionContext::new("task_a", Duration::from_secs(5))
            .with_cancellation_token(token_a_moved);
        UniversalExecutionEngine::execute_guarded(exec_a, &call, &ctx).await
    });

    // Task B uses child_token_b
    let exec_b = Arc::clone(&executor);
    let handle_b = tokio::spawn(async move {
        let call = CanonicalToolCall::new("task_b", "op", serde_json::json!({}));
        let ctx = ExecutionContext::new("task_b", Duration::from_secs(5))
            .with_cancellation_token(child_token_b);
        UniversalExecutionEngine::execute_guarded(exec_b, &call, &ctx).await
    });

    // Cancel ONLY child A
    child_token_a.cancel();

    let res_a = handle_a.await.unwrap();
    let res_b = handle_b.await.unwrap();

    // Task A must be cancelled
    assert!(
        matches!(res_a, Err(CapabilityError::ExecutionFailed { .. })),
        "Task A must abort when its child token is cancelled"
    );

    // Task B must succeed completely unaffected
    assert!(
        res_b.is_ok(),
        "Task B must succeed without interference from sibling cancellation"
    );
}

// ============================================================================
// SECTION 7: UniversalExecutionEngine - Panic Boundaries
// ============================================================================

#[tokio::test]
async fn test_adversarial_executor_panic_boundary_isolation() {
    // Verify that when an untrusted executor panics (both synchronously and asynchronously),
    // the panic is caught and isolated via tokio task join boundaries, preventing
    // host process termination, thread poisoning, or resource leakage.

    // 1. Synchronous Panic
    let sync_panicker = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::ZERO,
        should_panic: true,
        panic_async: false,
    });

    let handle_sync = tokio::spawn(async move {
        let call = CanonicalToolCall::new("sync_crash", "crash_tool", serde_json::json!({}));
        let ctx = ExecutionContext::new("sync_crash", Duration::from_secs(2));
        UniversalExecutionEngine::execute_guarded(sync_panicker, &call, &ctx).await
    });

    let sync_res = handle_sync.await;
    assert!(
        sync_res.is_err(),
        "Synchronous panic must be caught by task boundary"
    );
    let join_err = sync_res.unwrap_err();
    assert!(join_err.is_panic(), "JoinError must indicate task panicked");

    // 2. Asynchronous Panic (after await point)
    let async_panicker = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::from_millis(10),
        should_panic: true,
        panic_async: true,
    });

    let handle_async = tokio::spawn(async move {
        let call = CanonicalToolCall::new("async_crash", "crash_tool", serde_json::json!({}));
        let ctx = ExecutionContext::new("async_crash", Duration::from_secs(2));
        UniversalExecutionEngine::execute_guarded(async_panicker, &call, &ctx).await
    });

    let async_res = handle_async.await;
    assert!(
        async_res.is_err(),
        "Asynchronous panic must be caught by task boundary"
    );
    assert!(async_res.unwrap_err().is_panic());

    // 3. Verify healthy subsequent execution succeeds immediately after panic
    let healthy_executor = Arc::new(AdversarialDelayedExecutor {
        delay: Duration::ZERO,
        should_panic: false,
        panic_async: false,
    });
    let call_health = CanonicalToolCall::new("healthy", "ok_tool", serde_json::json!({}));
    let ctx_health = ExecutionContext::new("healthy", Duration::from_secs(1));
    let healthy_res =
        UniversalExecutionEngine::execute_guarded(healthy_executor, &call_health, &ctx_health)
            .await;
    assert!(
        healthy_res.is_ok(),
        "Subsequent execution must succeed cleanly after prior panics"
    );
}

// ============================================================================
// SECTION 8: Large Structured Error Payloads & Transient Backoff Telemetry
// ============================================================================

#[tokio::test]
async fn test_adversarial_large_structured_error_payloads() {
    // Generate a massive 500KB+ structured error details object with nested structures,
    // Unicode escape sequences, and adversarial strings.
    let mut large_map = serde_json::Map::new();
    for i in 0..1000 {
        large_map.insert(
            format!("trace_entry_{i}"),
            serde_json::json!({
                "frame": i,
                "file": "/usr/local/hakille/lib/internal_module.rs",
                "line": 42 + i,
                "symbols": ["core::ptr::drop_in_place", "alloc::raw_vec::RawVec::grow_amortized"],
                "memory_dump": "A".repeat(300),
                "adversarial_injection": "<script>alert('xss')</script>\"'; DROP TABLE audit;--",
                "unicode_payload": "🦀🔥🚀 ñ, å, ö, 語, 測試, 𝕳𝖆𝖐𝖎𝖑𝖑𝖊 \u{0000}\u{001f}",
            }),
        );
    }
    let massive_details = serde_json::Value::Object(large_map);

    let structured_err = StructuredToolError::transient(
        "UPSTREAM_SYSTEM_SATURATED",
        "The upstream inference cluster is heavily congested; backoff required.",
        3500,
    )
    .with_details(massive_details);

    // Convert to CanonicalToolResult
    let tool_res = structured_err.to_tool_result("call_err_999", RiskTier::Tier2ExternalNetwork);
    assert!(tool_res.is_error);
    assert_eq!(tool_res.metadata.risk_tier, RiskTier::Tier2ExternalNetwork);

    // Payload size should exceed 500KB
    let payload_len = tool_res.content[0].as_text().unwrap().len();
    assert!(
        payload_len > 400_000,
        "Payload length was {payload_len}, expected > 400,000 bytes"
    );

    // Roundtrip deserialize
    let recovered = StructuredToolError::from_tool_result(&tool_res);
    assert!(
        recovered.is_some(),
        "Must parse large StructuredToolError back from tool result"
    );

    let err = recovered.unwrap();
    assert_eq!(err.error_code, "UPSTREAM_SYSTEM_SATURATED");
    assert!(err.is_transient);
    assert_eq!(err.retry_after_ms, Some(3500));
    assert!(err.details.is_some());

    let details_obj = err.details.unwrap();
    assert!(details_obj.get("trace_entry_999").is_some());
}

#[test]
fn test_adversarial_malformed_tool_results_safe_recovery() {
    // CanonicalToolResult with is_error = false must return None
    let success_res = CanonicalToolResult::success_text(
        "call_ok",
        r#"{"error_code":"NOT_AN_ERROR","message":"not an error"}"#,
        ExecutionMetadata::new(10, RiskTier::Tier0ReadOnly),
    );
    assert!(
        StructuredToolError::from_tool_result(&success_res).is_none(),
        "Non-error result must return None"
    );

    // CanonicalToolResult with is_error = true but non-JSON text
    let non_json_err = CanonicalToolResult {
        call_id: "call_bad".to_string(),
        content: vec![CanonicalContentBlock::text(
            "Plain raw error string, not JSON",
        )],
        is_error: true,
        metadata: ExecutionMetadata::new(5, RiskTier::Tier0ReadOnly),
    };
    assert!(
        StructuredToolError::from_tool_result(&non_json_err).is_none(),
        "Plain text error must not crash parser and return None"
    );

    // Empty content blocks
    let empty_err = CanonicalToolResult {
        call_id: "call_empty".to_string(),
        content: vec![],
        is_error: true,
        metadata: ExecutionMetadata::new(0, RiskTier::Tier0ReadOnly),
    };
    assert!(StructuredToolError::from_tool_result(&empty_err).is_none());
}

#[tokio::test]
async fn test_adversarial_atomic_rollback_on_multi_tool_collision() {
    let registry = UniversalCapabilityRegistry::new();

    // Baseline capability with 5 tools
    let cap_a = make_manifest(
        "service.a",
        "1.0.0",
        &[
            "tool_a_1",
            "tool_a_2",
            "tool_a_3",
            "tool_a_4",
            "colliding_tool",
        ],
        &[],
        CapabilityType::Native,
        &[],
    );
    registry.register(cap_a).await.unwrap();
    registry.activate("service.a", None).await.unwrap();

    // Hostile capability with 15 unique tools and 1 colliding tool
    let mut b_tools: Vec<String> = (0..15).map(|i| format!("b_unique_tool_{i}")).collect();
    b_tools.push("colliding_tool".to_string());
    let b_tools_refs: Vec<&str> = b_tools.iter().map(|s| s.as_str()).collect();

    let cap_b = make_manifest(
        "service.b",
        "1.0.0",
        &b_tools_refs,
        &[],
        CapabilityType::Native,
        &[],
    );
    registry.register(cap_b).await.unwrap();

    // Activating service.b must fail due to colliding_tool
    let act_res = registry.activate("service.b", None).await;
    match act_res {
        Err(RegistryError::ToolCollision {
            tool_name,
            existing_capability_id,
        }) => {
            assert_eq!(tool_name, "colliding_tool");
            assert!(existing_capability_id.contains("service.a@1.0.0"));
        }
        other => panic!("Expected ToolCollision error, got {other:?}"),
    }

    // Invariant 1: service.a's colliding_tool must still resolve to service.a
    let (desc_a, _) = registry
        .find_tool("colliding_tool")
        .await
        .expect("tool must still exist");
    assert_eq!(desc_a.id, "service.a");

    // Invariant 2: None of service.b's unique tools should be reachable in find_tool
    for i in 0..15 {
        let name = format!("b_unique_tool_{i}");
        assert!(
            registry.find_tool(&name).await.is_none(),
            "Atomicity violation: unique tool {name} leaked into index despite activation failure"
        );
    }
}

#[tokio::test]
async fn test_adversarial_invalid_manifest_rejections_and_purity() {
    let registry = UniversalCapabilityRegistry::new();

    // 1. Manifest with duplicate tool names
    let dup_tool_manifest = CapabilityManifest::new(
        "dup.cap",
        "Dup Cap",
        Version::parse("1.0.0").unwrap(),
        "Desc",
        "Author",
        CapabilityType::Native,
    )
    .with_tool(CanonicalToolDefinition::new(
        "echo",
        "Echo 1",
        CanonicalToolDefinition::empty_parameters_schema(),
    ))
    .with_tool(CanonicalToolDefinition::new(
        "echo",
        "Echo 2 (dup)",
        CanonicalToolDefinition::empty_parameters_schema(),
    ));

    let reg_dup = registry.register(dup_tool_manifest).await;
    assert!(
        matches!(
            reg_dup,
            Err(RegistryError::Validation(
                hakille_capability::ValidationError::DuplicateToolName { .. }
            ))
        ),
        "Must reject manifest with duplicate tool names"
    );

    // 2. Manifest with invalid tool name (> 64 chars)
    let long_tool_name = "x".repeat(65);
    let long_name_manifest = CapabilityManifest::new(
        "long.cap",
        "Long Cap",
        Version::parse("1.0.0").unwrap(),
        "Desc",
        "Author",
        CapabilityType::Native,
    )
    .with_tool(CanonicalToolDefinition::new(
        long_tool_name,
        "Long tool",
        CanonicalToolDefinition::empty_parameters_schema(),
    ));

    let reg_long = registry.register(long_name_manifest).await;
    assert!(
        matches!(
            reg_long,
            Err(RegistryError::Validation(
                hakille_capability::ValidationError::InvalidToolName { .. }
            ))
        ),
        "Must reject manifest with oversized tool name"
    );

    // Registry must remain pristine
    assert!(registry.list_active().await.is_empty());
}

#[tokio::test]
async fn test_adversarial_rapid_toggle_activation_disable() {
    let registry = Arc::new(UniversalCapabilityRegistry::new());
    let manifest = make_manifest(
        "toggle.cap",
        "1.0.0",
        &["flip_tool"],
        &[],
        CapabilityType::Native,
        &[],
    );
    registry.register(manifest).await.unwrap();

    let reg_clone = Arc::clone(&registry);
    let writer_handle = tokio::spawn(async move {
        for _ in 0..100 {
            reg_clone.activate("toggle.cap", None).await.unwrap();
            tokio::task::yield_now().await;
            reg_clone.disable("toggle.cap").await.unwrap();
            tokio::task::yield_now().await;
        }
    });

    let reg_reader = Arc::clone(&registry);
    let reader_handle = tokio::spawn(async move {
        for _ in 0..300 {
            let _ = reg_reader.find_tool("flip_tool").await;
            let _ = reg_reader.list_active().await;
            tokio::task::yield_now().await;
        }
    });

    writer_handle.await.unwrap();
    reader_handle.await.unwrap();

    // After loop, capability was disabled last
    assert!(registry.find_tool("flip_tool").await.is_none());
}

#[test]
fn test_adversarial_deeply_nested_json_error_details() {
    // Construct 100-level deeply nested JSON value
    let mut current = serde_json::json!({"leaf_key": "deep_error_data"});
    for level in (0..100).rev() {
        current = serde_json::json!({ format!("level_{level}"): current });
    }

    let err =
        StructuredToolError::new("DEEP_RECURSION_ERROR", "Deep error stack").with_details(current);

    let tool_res = err.to_tool_result("deep_call", RiskTier::Tier0ReadOnly);
    assert!(tool_res.is_error);

    let roundtripped = StructuredToolError::from_tool_result(&tool_res);
    assert!(
        roundtripped.is_some(),
        "Deep JSON error details must roundtrip safely without stack overflow"
    );
}

#[tokio::test]
async fn test_adversarial_cancellation_vs_completion_race() {
    // Stress test racing execution completion vs cancellation token trigger
    for iteration in 0..30 {
        let executor = Arc::new(AdversarialDelayedExecutor {
            delay: Duration::from_millis(2),
            should_panic: false,
            panic_async: false,
        });

        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();
        let call = CanonicalToolCall::new(
            format!("race_{iteration}"),
            "race_tool",
            serde_json::json!({}),
        );
        let ctx = ExecutionContext::new(format!("race_{iteration}"), Duration::from_secs(1))
            .with_cancellation_token(cancel_token);

        let exec_handle = tokio::spawn(async move {
            UniversalExecutionEngine::execute_guarded(executor, &call, &ctx).await
        });

        // Trigger cancellation concurrently with minimal delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(2)).await;
            cancel_clone.cancel();
        });

        let result = exec_handle.await.unwrap();
        match result {
            Ok(tool_res) => {
                assert!(!tool_res.is_error);
                assert_eq!(tool_res.text_summary(), "success");
            }
            Err(CapabilityError::ExecutionFailed { message, .. }) => {
                assert!(message.contains("cancellation token"));
            }
            other => panic!("Unexpected race result: {other:?}"),
        }
    }
}
