# ADR-020: Hybrid Capability Discovery via Structured Filtering and Vector Ranking with Lexical Fallback

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: AI Research Lead, Information Retrieval Specialist
- **Target Module**: `crates/hakille-capability/src/discovery/`

## Context & Problem Statement

As Hakille scales to multiple capabilities across Native, MCP, Skills, and Agent plugins, injecting all tool definitions into every LLM prompt becomes unviable. Monolithic schema injection causes:
- Context window exhaustion and higher inference latency.
- "Lost-in-the-middle" reasoning degradation.
- Excessive token consumption per conversation turn.

Hakille requires a hybrid discovery engine combining structured security filtering with semantic vector ranking, along with an honest lexical fallback when embeddings are not available.

## Decision Drivers

- **Bounded Context Footprint**: Dynamic greedy knapsack packing to enforce token budgets (`token_budget`).
- **Two-Phase Hybrid Filtering**: Hard scope, identity, tag, and risk tier filtering first, followed by semantic ranking.
- **Honest Fallback**: When vector embeddings are not provided, perform lexical token matching rather than dummy stubs or simulated scores.
- **Fail-Closed Security**: Expired caller tokens and unauthorized risk tiers are pruned during Phase 1 pre-filtering.

## Considered Alternatives

1. **Static Inlining of All Tools**:
   - *Cons*: Exhausts context budget quickly.
   - *Verdict*: Rejected.
2. **Pure Lexical Search Only**:
   - *Cons*: Misses semantic synonyms when embeddings are available.
   - *Verdict*: Used as an honest fallback when embeddings are omitted.
3. **Two-Phase Hybrid Filtering + Cosine Ranking with Lexical Fallback**:
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

The hybrid discovery pipeline operates in three phases:
1. **Phase 1: Structured Pre-Filtering**:
   - Prunes tools if the caller Biscuit token is expired.
   - Filters by allowed `CapabilityType` (Native, MCP, Skill, Agent, Custom).
   - Filters by required metadata tags.
   - Enforces `max_risk_tier` ceiling.
2. **Phase 2: Semantic & Lexical Ranking**:
   - If query and tool embeddings are provided: calculates L2-normalized cosine similarity $\text{sim}(q, t) \ge \text{min\_similarity}$.
   - If query embeddings are omitted: evaluates lexical token overlap between query terms and `tool.name` + `tool.description`, sorting by relevance.
3. **Phase 3: Dynamic Prompt Budget Optimizer**:
   - Greedy knapsack packing ordered by score descending.
   - Caps total prompt tokens strictly within `token_budget` and returns Top-$K$ tools.

## Consequences

- **Positive**: Substantially reduced context window footprint; honest behavior across environments with or without embedding models.
- **Negative**: Lexical fallback lacks deep synonym understanding when embeddings are absent.
