# ADR-003: 3-Tier Progressive Skills Loading

- **Status**: Ratified
- **Date**: 2026-11-07
- **Deciders**: AI Cognitive Architect, Context Optimization Lead
- **Target Module**: `crates/hakille-capability/src/adapters/skills/`

## Context & Problem Statement

Agent Skills (`SKILL.md`) provide domain-specific instructions, heuristic checklists, and bundled scripts. Monolithically stuffing dozens of full skill texts into LLM prompts causes severe problems:
1. Wastes tens of thousands of tokens per conversation turn.
2. Increases inference latency and cost.
3. Degrades model reasoning via "needle-in-a-haystack" distraction.
4. Leaves local files vulnerable to path traversal attacks when scripts are fetched dynamically.

## Decision Drivers

- **Token Efficiency**: The global catalog index must cost minimal tokens (~40 tokens per skill).
- **Just-in-Time Knowledge**: In-depth cognitive instructions should only enter the prompt when an intent matches.
- **Lazy Resource I/O**: Executable scripts and reference files must only be loaded on demand.
- **Sandboxed File Access**: Prevent `../../` path traversal when retrieving skill resources.

## Considered Alternatives

1. **Monolithic System Prompt Injection**:
   - *Cons*: Instantly exhausts context budgets; high latency.
   - *Verdict*: Rejected.
2. **Pure Vector RAG without Metadata**:
   - *Cons*: Fails to notify the model that the skill exists or what parameters it expects before the model formulates an intent.
   - *Verdict*: Rejected.
3. **3-Tier Progressive Loading Pipeline**:
   - *Tier 1*: Frontmatter Catalog Indexing (statically cached metadata).
   - *Tier 2*: Runtime Body Injection (on intent match or tool selection).
   - *Tier 3*: Lazy Resource Fetching (sandboxed on-demand I/O).
   - *Verdict*: **Adopted**.

## Decision & Technical Specification

The 3-tier loading architecture consists of:
1. **Tier 1 (Discovery & Catalog Index)**:
   - Parses YAML frontmatter (`name`, `description`, `parameters`, `permissions`).
   - Indexes metadata in `UniversalCapabilityRegistry` and vector index.
   - Injects concise summary catalog into system prompt (~40 tokens/skill).
2. **Tier 2 (Runtime Knowledge Injection)**:
   - When user query or model intent matches a skill, injects Markdown instructions and heuristics into the active turn.
   - Governed by LRU eviction (maximum 3–5 active skills per turn).
3. **Tier 3 (Lazy Resource Fetching)**:
   - Scripts and reference files loaded via `get_skill_resources(name, path)`.
   - Path traversal guard: checks that `fs::canonicalize(root.join(path))` starts with `fs::canonicalize(root)`. Rejects traversal attempts with `CapabilityError`.

## Consequences

- **Positive**: 90%+ reduction in initial prompt token consumption; eliminates context saturation; defends against directory traversal.
- **Negative**: Adds an asynchronous file read step when external resources are fetched.
