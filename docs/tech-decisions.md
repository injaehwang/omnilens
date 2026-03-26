# Technical Decisions

## ADR-001: Rust as Primary Language

**Status**: Accepted
**Date**: 2026-03-26

**Context**: Need a systems language for a code analysis engine that must be fast, memory-efficient, and handle complex type systems (AST manipulation, graph algorithms, IR design).

**Decision**: Rust for the core engine. Go for the cloud backend (Phase 4). TypeScript for IDE extensions.

**Rationale**:
- `enum` + `pattern matching` is ideal for compiler/IR engineering
- Zero-cost abstractions for performance-critical graph traversal
- tree-sitter has first-class Rust bindings
- eBPF tooling (`aya`) is mature in Rust
- WASM ecosystem (wasmtime) is Rust-native
- Single binary distribution without runtime dependencies
- Every recent successful analysis tool chose Rust: ruff, oxc, swc, turbopack, biome

**Trade-offs**:
- Slower compilation (mitigated by workspace structure + incremental builds)
- Higher contributor barrier (mitigated by clean API boundaries + good docs)

---

## ADR-002: tree-sitter for Parsing

**Status**: Accepted
**Date**: 2026-03-26

**Context**: Need multi-language parsing that is fast, incremental, and error-tolerant.

**Decision**: Use tree-sitter as the parsing layer for all language frontends.

**Rationale**:
- Supports 100+ languages with community-maintained grammars
- Incremental parsing: re-parse only changed regions
- Error-tolerant: produces partial ASTs for broken code
- Battle-tested in production (GitHub, Neovim, Zed, Helix)
- C ABI with excellent Rust bindings

**Trade-offs**:
- tree-sitter grammars capture syntax, not semantics — we need our own semantic layer (USIR)
- Some grammars are better maintained than others
- Type information requires additional resolution beyond tree-sitter

---

## ADR-003: Custom Universal Semantic IR (USIR)

**Status**: Accepted
**Date**: 2026-03-26

**Context**: Need a language-independent representation for cross-language analysis.

**Decision**: Design a custom mid-level IR rather than using existing IRs.

**Alternatives considered**:
- **LLVM IR**: Too low-level, loses semantic information (function names, types, module structure)
- **WASM**: Too execution-focused, not analysis-friendly
- **Language-specific ASTs**: No cross-language querying possible
- **LSP protocol types**: Too coarse-grained, designed for editor features

**Rationale**:
- USIR sits between AST (too detailed) and call-graph (too abstract)
- Captures: function signatures, call relationships, data flow, type hierarchies, module boundaries, API endpoints
- Designed for graph queries, not execution
- Extensible: new node/edge types can be added per-language without breaking core

---

## ADR-004: petgraph for Graph Storage

**Status**: Accepted
**Date**: 2026-03-26

**Context**: Need a fast in-memory graph database for the semantic graph.

**Decision**: Use `petgraph::StableDiGraph` as the core graph structure.

**Rationale**:
- Stable node/edge indices (critical for incremental updates)
- Rich algorithm library (BFS, DFS, Dijkstra, topological sort, SCC)
- Zero allocation overhead vs external graph databases
- Serializable with bincode for persistence
- Used successfully by cargo, rustc, and other Rust tools

**Trade-offs**:
- In-memory: limited by available RAM (mitigated by lazy loading for huge codebases)
- No built-in query language (we build OmniQL on top)
- Not distributed (acceptable for single-machine analysis)

---

## ADR-005: Content-Addressed Storage

**Status**: Accepted
**Date**: 2026-03-26

**Context**: Need efficient incremental indexing that avoids redundant work.

**Decision**: Use content-addressed storage (SHA-256 hash of source content) for USIR objects.

**Rationale**:
- Same model as git: if content hasn't changed, hash matches, skip re-analysis
- Natural deduplication (identical files across branches)
- Easy cache invalidation: hash mismatch → re-parse
- Enables future distributed caching (share analysis results across machines)

---

## ADR-006: redb for Embedded Key-Value Storage

**Status**: Accepted
**Date**: 2026-03-26

**Context**: Need a persistent key-value store for the index and metadata.

**Decision**: Use `redb` as the embedded database.

**Alternatives considered**:
- **sled**: Development stalled, reliability concerns
- **SQLite**: Good but overkill, FFI dependency
- **RocksDB**: Heavy C++ dependency, complex build
- **Custom**: Unnecessary when redb exists

**Rationale**:
- Pure Rust: no C dependencies, simple cross-compilation
- ACID transactions
- Zero-copy reads via memory mapping
- Small binary size impact
- Active development and maintenance

---

## ADR-007: WASM for Plugin System

**Status**: Planned (Phase 3)
**Date**: 2026-03-26

**Context**: Need extensibility for community-contributed analyzers and language frontends.

**Decision**: Plugins are compiled to WASM and executed via wasmtime.

**Rationale**:
- Sandboxed execution: plugins cannot crash the host
- Language-agnostic: plugins can be written in Rust, C, Go, AssemblyScript, etc.
- Deterministic: same input → same output
- Growing ecosystem and tooling (WASI, component model)

**Trade-offs**:
- Performance overhead vs native plugins (~10-30%)
- Limited host API surface (by design, for security)
- WASI ecosystem still maturing
