# ADR-006: Hybrid Capability Discovery via MiniLM 384-dim HNSW Vector Index & Structured Filtering

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: AI Research Lead, Information Retrieval Specialist
- **Target Module**: `crates/hakille-capability/src/discovery/`

## Context & Problem Statement

As Hakille scales to hundreds or thousands of capabilities across Native, MCP, Skills, and WASM plugins, stuffing all tool definitions into LLM prompts becomes unviable. Monolithic schema injection causes:
- Context window exhaustion and high inference latency.
- "Lost-in-the-middle" reasoning degradation.
- High token costs per conversation turn.
Hakille already includes MiniLM-L6-v2 ONNX embeddings (384 dimensions) in `crates/hakille-inference/src/embed_onnx.rs` and the `instant-distance 0.6` HNSW vector dependency in `crates/hakille-memory`.
Hakille requires a hybrid discovery engine combining structured attribute filtering with semantic vector search to dynamically select the Top-$K$ relevant tools for any turn.

## Decision Drivers

- **Bounded Context Footprint**: Limit injected tools to Top-$K$ (default $K=5$, max 10), capping token consumption to ~500–1,000 tokens.
- **Sub-Millisecond Retrieval**: Fast Approximate Nearest Neighbor (ANN) search (< 1 ms for 10,000 tools).
- **Two-Phase Hybrid Filtering**: Hard scope, namespace, and risk tier filtering first, followed by semantic vector similarity.
- **Dynamic Synchronization**: Instant index updates upon tool registration, activation, or removal.

## Considered Alternatives

1. **Pure Lexical Search (BM25 / Regex)**:
   - *Cons*: Misses semantic synonyms (e.g. user asks "check the weather", but tool is named "get_meteorological_forecast").
   - *Verdict*: Insufficient as sole discovery mechanism.
2. **Exhaustive Brute-Force Cosine Scan**:
   - *Cons*: Scales linearly $O(N \cdot D)$. At $N=10,000$ tools, latency exceeds budget.
   - *Verdict*: Acceptable only as a fallback for $N < 100$.
3. **Hybrid Filtered HNSW Index with MiniLM 384-dim Embeddings**:
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

The hybrid discovery pipeline operates in three phases:
1. **Phase 1: Structured Attribute & Scope Pre-Filter**:
   - Excludes tools requiring ungranted Biscuit scopes.
   - Filters by active caller identity, namespace, and maximum allowable risk tier.
   - Produces candidate subset $C \subseteq \text{All Tools}$.
2. **Phase 2: Semantic Embedding & HNSW Vector Retrieval**:
   - Generates 384-dimensional query vector via MiniLM-L6-v2.
   - Queries HNSW index (`instant-distance`) over candidate embeddings.
   - Evaluates cosine similarity: $\text{sim}(q, t) = q \cdot t$ (L2-normalized).
3. **Phase 3: Dynamic Top-K Selection & Schema Translation**:
   - Filters candidates by minimum similarity threshold ($\theta \ge 0.65$).
   - Returns Top-5 most relevant `CanonicalToolDefinition`s.
   - Passes tools to target Provider Schema Translator for injection into LLM context.

## Consequences

- **Positive**: 98% reduction in context window token consumption; sub-millisecond discovery; semantic matching eliminates phrasing mismatches.
- **Negative**: Requires computing MiniLM embeddings for tools at registration time (~5 ms per tool).
