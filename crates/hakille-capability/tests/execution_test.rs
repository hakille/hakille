//! Integration tests for Universal Execution Engine.

use async_trait::async_trait;
use hakille_capability::error::CapabilityError;
use hakille_capability::types::{
    CanonicalToolCall, CanonicalToolResult, ExecutionMetadata, RiskTier,
};
use hakille_capability::{
    CapabilityExecutor, ExecutionContext, StructuredToolError, UniversalExecutionEngine,
};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Test executor that simulates normal execution, long delay (for timeout testing),
/// or domain failure.
struct TestTimedExecutor {
    delay: Duration,
    fail_with_domain_error: bool,
}

#[async_trait]
impl CapabilityExecutor for TestTimedExecutor {
    async fn execute(
        &self,
        call: &CanonicalToolCall,
        context: &ExecutionContext,
    ) -> Result<CanonicalToolResult, CapabilityError> {
        if self.delay > Duration::ZERO {
            tokio::select! {
                _ = context.cancellation_token.cancelled() => {
                    return Err(CapabilityError::ExecutionFailed {
                        tool_name: call.tool_name.clone(),
                        message: "Child executor aborted by cancellation token".to_string(),
                    });
                }
                _ = tokio::time::sleep(self.delay) => {}
            }
        }

        if self.fail_with_domain_error {
            let err = StructuredToolError::transient(
                "UPSTREAM_RATE_LIMIT",
                "Too many requests to provider endpoint",
                500,
            )
            .with_details(serde_json::json!({"limit": 60, "reset_s": 5}));
            return Ok(err.to_tool_result(&call.call_id, context.risk_tier));
        }

        Ok(CanonicalToolResult::success_text(
            &call.call_id,
            "Execution completed successfully",
            ExecutionMetadata::new(0, context.risk_tier),
        ))
    }
}

#[tokio::test]
async fn test_successful_guarded_execution() {
    let executor = Arc::new(TestTimedExecutor {
        delay: Duration::from_millis(5),
        fail_with_domain_error: false,
    });
    let call = CanonicalToolCall::new("call_01", "test_tool", serde_json::json!({}));
    let ctx = ExecutionContext::new("call_01", Duration::from_millis(500))
        .with_risk_tier(RiskTier::Tier1StateMutating);

    let res = UniversalExecutionEngine::execute_guarded(executor, &call, &ctx).await;
    assert!(res.is_ok());

    let tool_res = res.unwrap();
    assert!(!tool_res.is_error);
    assert_eq!(tool_res.metadata.risk_tier, RiskTier::Tier1StateMutating);
    assert_eq!(tool_res.text_summary(), "Execution completed successfully");
}

#[tokio::test]
async fn test_hard_timeout_preemption() {
    let executor = Arc::new(TestTimedExecutor {
        delay: Duration::from_millis(200),
        fail_with_domain_error: false,
    });
    let call = CanonicalToolCall::new("call_02", "slow_tool", serde_json::json!({}));
    let ctx = ExecutionContext::new("call_02", Duration::from_millis(25));

    let res = UniversalExecutionEngine::execute_guarded(executor, &call, &ctx).await;
    match res {
        Err(CapabilityError::Timeout { timeout_ms }) => {
            assert_eq!(timeout_ms, 25);
        }
        other => panic!("Expected Timeout error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_cooperative_cancellation_abort() {
    let executor = Arc::new(TestTimedExecutor {
        delay: Duration::from_millis(300),
        fail_with_domain_error: false,
    });
    let cancel_token = CancellationToken::new();
    let call = CanonicalToolCall::new("call_03", "cancelled_tool", serde_json::json!({}));
    let ctx = ExecutionContext::new("call_03", Duration::from_secs(5))
        .with_cancellation_token(cancel_token.clone());

    // Spawn execution in background and cancel immediately
    let exec_task = tokio::spawn(async move {
        UniversalExecutionEngine::execute_guarded(executor, &call, &ctx).await
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    cancel_token.cancel();

    let res = exec_task.await.unwrap();
    match res {
        Err(CapabilityError::ExecutionFailed { tool_name, message }) => {
            assert_eq!(tool_name, "cancelled_tool");
            assert!(message.contains("cancellation token"));
        }
        other => panic!("Expected cancellation ExecutionFailed error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_structured_tool_error_roundtrip() {
    let executor = Arc::new(TestTimedExecutor {
        delay: Duration::ZERO,
        fail_with_domain_error: true,
    });
    let call = CanonicalToolCall::new("call_04", "api_call", serde_json::json!({}));
    let ctx = ExecutionContext::new("call_04", Duration::from_secs(1))
        .with_risk_tier(RiskTier::Tier2ExternalNetwork);

    let res = UniversalExecutionEngine::execute_guarded(executor, &call, &ctx).await;
    assert!(res.is_ok());

    let tool_res = res.unwrap();
    assert!(tool_res.is_error);

    // Extract structured error from result
    let structured = StructuredToolError::from_tool_result(&tool_res);
    assert!(structured.is_some());
    let err = structured.unwrap();
    assert_eq!(err.error_code, "UPSTREAM_RATE_LIMIT");
    assert!(err.is_transient);
    assert_eq!(err.retry_after_ms, Some(500));
    assert!(err.details.is_some());
}
