# omnilens Architecture

## Overview

omnilens is an **AI-native code verification engine**. It combines semantic code analysis, invariant discovery, behavioral contract inference, and property-based test synthesis to verify code — especially AI-generated code — at the speed it's produced.

Each layer is designed to be independently testable, incrementally buildable, and extensible via WASM plugins.

```
                         ┌─────────────┐
                         │   User      │
                         └──────┬──────┘
                                │
                    ┌───────────▼───────────┐
                    │   Interface Layer      │
                    │  CLI / LSP / API       │
                    └───────────┬───────────┘
                                │
                    ┌───────────▼───────────┐
                    │   Query Engine         │
                    │   OmniQL Parser +      │
                    │   Execution Engine     │
                    └───────────┬───────────┘
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                   │
    ┌─────────▼──────┐ ┌───────▼────────┐ ┌───────▼────────┐
    │ Impact Analyzer │ │ Pattern Matcher│ │ Test Generator │
    │ (graph traversal│ │ (USIR queries) │ │ (symbolic exec)│
    │  + scoring)     │ │                │ │                │
    └─────────┬──────┘ └───────┬────────┘ └───────┬────────┘
              │                 │                   │
              └─────────────────┼───────────────────┘
                                │
                    ┌───────────▼───────────┐
                    │   Semantic Graph       │
                    │   (in-memory graph DB) │
                    │   Nodes: functions,    │
                    │   types, modules, APIs │
                    │   Edges: calls, uses,  │
                    │   implements, imports   │
                    └───────────┬───────────┘
                                │
                    ┌───────────▼───────────┐
                    │   Universal Semantic   │
                    │   IR (USIR)            │
                    │   Language-independent │
                    │   representation       │
                    └───────────┬───────────┘
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                   │
    ┌─────────▼──────┐ ┌───────▼────────┐ ┌───────▼────────┐
    │ Rust Frontend  │ │ TS/JS Frontend │ │ Python Frontend│
    │ (tree-sitter)  │ │ (tree-sitter)  │ │ (tree-sitter)  │
    └─────────┬──────┘ └───────┬────────┘ └───────┬────────┘
              │                 │                   │
              └─────────────────┼───────────────────┘
                                │
                    ┌───────────▼───────────┐
                    │   Indexing Engine      │
                    │   (content-addressed,  │
                    │    git-aware,          │
                    │    incremental)        │
                    └───────────┬───────────┘
                                │
                    ┌───────────▼───────────┐
                    │   Storage Layer        │
                    │   (mmap + sled/redb)   │
                    └───────────────────────┘
```

## Layer Details

### 1. Storage Layer

**Responsibility**: Persist indexed data across sessions with fast startup.

```
storage/
├── objects/          # content-addressed blobs (hash → USIR node)
│   ├── ab/
│   │   ├── cdef1234...
│   │   └── ...
├── graph.bin         # serialized petgraph (bincode)
├── index.bin         # file path → object hash mapping
├── runtime.bin       # runtime profiling overlay data
└── meta.json         # project metadata, language stats
```

**Key decisions**:
- Content-addressed storage (SHA-256 of source + parse config) for deduplication
- Memory-mapped files for large graphs (mmap)
- `redb` as embedded key-value store (pure Rust, ACID, zero-copy reads)
- Incremental: only re-index files whose content hash changed since last run

### 2. Indexing Engine

**Responsibility**: Watch filesystem, detect changes, trigger re-parsing.

```rust
pub struct Indexer {
    /// Maps file paths to their content hashes
    file_index: HashMap<PathBuf, ContentHash>,
    /// Git integration for change detection
    git_differ: GitDiffer,
    /// Parallel parsing coordinator
    parse_pool: rayon::ThreadPool,
}

impl Indexer {
    /// Fast path: use git diff to find changed files
    /// Fallback: content-hash comparison for non-git projects
    pub fn detect_changes(&self) -> Vec<ChangedFile>;

    /// Parse only changed files, update graph incrementally
    pub fn incremental_update(&mut self, changes: &[ChangedFile]) -> UpdateResult;
}
```

**Performance targets**:
- 100K LOC project: initial index < 5 seconds
- Incremental update (single file change): < 100ms
- 1M LOC project: initial index < 60 seconds

### 3. Language Frontends

**Responsibility**: Parse source code into USIR using tree-sitter.

Each frontend implements the `LanguageFrontend` trait:

```rust
pub trait LanguageFrontend: Send + Sync {
    /// Supported file extensions
    fn extensions(&self) -> &[&str];

    /// Parse a single file into USIR nodes
    fn parse_file(&self, path: &Path, source: &[u8]) -> Result<Vec<UsirNode>>;

    /// Extract cross-file references (imports, exports)
    fn extract_references(&self, source: &[u8]) -> Result<Vec<Reference>>;

    /// Language-specific type resolution
    fn resolve_types(&self, node: &UsirNode, context: &TypeContext) -> Result<ResolvedType>;
}
```

**Implementation strategy**:
- tree-sitter for parsing (battle-tested, incremental, all languages)
- Each frontend is a separate crate for modularity
- WASM plugin API allows community-contributed frontends

### 4. Universal Semantic IR (USIR)

**Responsibility**: Language-independent representation of code semantics.

This is the **core innovation** of omnilens. USIR captures:

```rust
/// A node in the USIR graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsirNode {
    /// A callable unit (function, method, closure, lambda)
    Function {
        id: NodeId,
        name: QualifiedName,
        params: Vec<Param>,
        return_type: TypeRef,
        visibility: Visibility,
        body: Vec<UsirStatement>,
        source_span: SourceSpan,
    },

    /// A data structure (struct, class, interface, protocol)
    DataType {
        id: NodeId,
        name: QualifiedName,
        kind: DataTypeKind, // Struct, Class, Interface, Enum, Union
        fields: Vec<Field>,
        methods: Vec<NodeId>, // references to Function nodes
        implements: Vec<TypeRef>,
        source_span: SourceSpan,
    },

    /// A module/namespace boundary
    Module {
        id: NodeId,
        name: QualifiedName,
        exports: Vec<NodeId>,
        imports: Vec<Import>,
        source_span: SourceSpan,
    },

    /// An API endpoint (HTTP route, gRPC method, GraphQL resolver)
    ApiEndpoint {
        id: NodeId,
        protocol: ApiProtocol,
        method: Option<HttpMethod>,
        path: String,
        handler: NodeId, // reference to Function node
        source_span: SourceSpan,
    },
}

/// Edges between USIR nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsirEdge {
    /// Function A calls Function B
    Calls {
        caller: NodeId,
        callee: NodeId,
        call_site: SourceSpan,
        condition: Option<Condition>, // under what conditions
    },

    /// Type A references Type B
    References { from: NodeId, to: NodeId },

    /// Type A implements Interface B
    Implements { implementor: NodeId, interface: NodeId },

    /// Data flows from A to B
    DataFlow {
        source: NodeId,
        sink: NodeId,
        taint: TaintKind, // UserInput, Database, FileSystem, Network
    },

    /// Module A imports from Module B
    Imports { importer: NodeId, imported: NodeId },
}
```

**Design principles**:
- **Semantic, not syntactic**: `for`, `while`, `map`, `forEach` all become iteration constructs
- **Typed but flexible**: type information is best-effort (critical for dynamic languages)
- **Taint-aware**: data flow edges carry taint labels for security analysis
- **Condition-aware**: call edges record conditions for precise impact analysis

### 5. Semantic Graph

**Responsibility**: In-memory graph database over USIR nodes and edges.

```rust
pub struct SemanticGraph {
    /// The core graph structure (petgraph)
    graph: StableDiGraph<UsirNode, UsirEdge>,

    /// Fast lookup indices
    name_index: HashMap<QualifiedName, NodeId>,
    file_index: HashMap<PathBuf, Vec<NodeId>>,
    type_index: HashMap<TypeRef, Vec<NodeId>>,

    /// Runtime overlay (optional, from trace data)
    runtime_weights: Option<RuntimeOverlay>,
}

impl SemanticGraph {
    /// Forward impact: "what does this node affect?"
    pub fn impact_forward(&self, node: NodeId, depth: usize) -> ImpactResult;

    /// Reverse impact: "what affects this node?"
    pub fn impact_reverse(&self, node: NodeId, depth: usize) -> ImpactResult;

    /// Pattern matching query execution
    pub fn query(&self, pattern: &UsirPattern) -> Vec<QueryMatch>;

    /// Shortest path between two nodes
    pub fn path_between(&self, from: NodeId, to: NodeId) -> Option<Vec<NodeId>>;

    /// Subgraph extraction for a given scope
    pub fn subgraph(&self, root: NodeId, depth: usize) -> SemanticGraph;
}
```

### 6. Analysis Engines

#### Impact Analyzer

```rust
pub struct ImpactAnalyzer<'g> {
    graph: &'g SemanticGraph,
    scorer: ImpactScorer,
}

pub struct ImpactResult {
    pub direct: Vec<ImpactedNode>,      // immediate callers/dependents
    pub transitive: Vec<ImpactedNode>,  // full propagation
    pub api_surface: Vec<ApiEndpoint>,  // affected public APIs
    pub test_coverage: CoverageReport,  // which paths are tested
    pub risk_score: f64,                // 0.0 - 1.0
}

pub struct ImpactedNode {
    pub node: NodeId,
    pub distance: usize,              // hops from changed node
    pub path: Vec<NodeId>,            // shortest path to change
    pub confidence: f64,              // how certain is this impact
    pub runtime_frequency: Option<u64>, // how often this path executes
}
```

**Scoring algorithm**:
- Base score: graph distance (closer = higher impact)
- Multipliers: runtime frequency, test coverage gaps, API visibility
- Condition analysis: conditional calls reduce confidence score

#### Pattern Matcher

Executes OmniQL queries against the semantic graph:

```
// OmniQL examples
FIND functions WHERE calls(db.query) AND NOT handles(Error)
FIND dataflow FROM tag(UserInput) TO tag(SqlQuery) WITHOUT sanitize()
FIND functions WHERE complexity > 20 AND test_coverage < 0.5
```

#### Test Generator

Uses constraint-based test generation:

```rust
pub struct TestGenerator<'g> {
    graph: &'g SemanticGraph,
    solver: ConstraintSolver,  // Z3 or custom solver
}

impl TestGenerator {
    /// Analyze function paths and generate boundary tests
    pub fn generate(&self, target: NodeId, strategy: TestStrategy) -> Vec<GeneratedTest>;
}

pub enum TestStrategy {
    Boundary,      // min/max/zero/empty for each parameter
    ErrorPaths,    // trigger every error return path
    RacePaths,     // concurrent access patterns
    DataFlow,      // trace tainted data through function
    Coverage,      // maximize branch coverage
}
```

### 7. Query Engine (OmniQL)

**Responsibility**: Parse and execute OmniQL queries.

```
OmniQL Grammar (simplified):

query     := FIND target WHERE condition (AND condition)*
target    := "functions" | "types" | "modules" | "dataflow" | "apis"
condition := predicate "(" args ")"
           | field operator value
           | NOT condition
predicate := "calls" | "handles" | "implements" | "imports" | "tag"
operator  := ">" | "<" | "=" | "!=" | "~" (regex match)
```

OmniQL compiles to a graph traversal plan (similar to a SQL query planner):

```rust
pub struct QueryPlan {
    pub root_scan: ScanStrategy,    // Full scan vs index lookup
    pub filters: Vec<FilterStep>,    // Predicate evaluation order
    pub joins: Vec<JoinStep>,        // Graph traversals needed
    pub projection: Projection,      // What to return
}
```

### 8. Runtime Profiler

**Responsibility**: Collect runtime data and overlay onto static graph.

```rust
pub struct RuntimeOverlay {
    /// Call frequency: how many times each edge was traversed
    pub call_counts: HashMap<(NodeId, NodeId), u64>,
    /// Execution time per function
    pub exec_times: HashMap<NodeId, Duration>,
    /// Memory allocation per function
    pub alloc_sizes: HashMap<NodeId, usize>,
    /// Actual types at dynamic dispatch sites
    pub resolved_dispatch: HashMap<NodeId, Vec<TypeRef>>,
}
```

**Platform-specific backends**:
- Linux: eBPF via `aya` crate (uprobe/USDT)
- macOS: DTrace integration
- Windows: ETW (Event Tracing for Windows)
- Fallback: instrumentation-based (source-level injection)

### 9. Interface Layer

#### CLI

```
omnilens <COMMAND>

Commands:
  init        Initialize omnilens in current project
  index       Build/update the semantic index
  impact      Analyze impact of a change
  query       Run an OmniQL query
  trace       Attach runtime profiler
  testgen     Generate tests for uncovered paths
  graph       Export semantic graph (DOT, JSON, interactive HTML)
  serve       Start LSP server for IDE integration
  plugin      Manage WASM plugins

Global Options:
  --format    Output format: text, json, sarif
  --color     Color output: auto, always, never
  --verbose   Increase verbosity (-v, -vv, -vvv)
```

#### LSP Protocol Extension

Standard LSP + custom methods:

```
omnilens/impact        → Impact analysis for symbol at cursor
omnilens/query         → OmniQL query execution
omnilens/graph         → Subgraph around cursor position
omnilens/testgen       → Generate tests for function at cursor
omnilens/runtimeData   → Runtime overlay for current file
```

## Data Flow

### Indexing Flow

```
File Change Detected
        │
        ▼
  Git Diff / Hash Compare
        │
        ▼
  tree-sitter Parse (parallel, per-file)
        │
        ▼
  Language Frontend → USIR Nodes
        │
        ▼
  Cross-file Reference Resolution
        │
        ▼
  Graph Update (incremental)
        │
        ▼
  Storage Persist (content-addressed)
```

### Query Flow

```
User Query (CLI/LSP/API)
        │
        ▼
  OmniQL Parse → AST
        │
        ▼
  Query Plan Optimization
        │
        ▼
  Graph Traversal Execution
        │
        ▼
  Runtime Data Overlay (if available)
        │
        ▼
  Result Scoring & Ranking
        │
        ▼
  Output Formatting (text/json/sarif)
```

## Extension Points

### WASM Plugin API

```rust
/// Plugins implement this interface (compiled to WASM)
pub trait OmnilensPlugin {
    /// Plugin metadata
    fn manifest(&self) -> PluginManifest;

    /// Custom analysis pass over USIR nodes
    fn analyze(&self, graph: &ReadOnlyGraph) -> Vec<Diagnostic>;

    /// Custom OmniQL predicates
    fn predicates(&self) -> Vec<PredicateDefinition>;

    /// Custom output formatters
    fn formatters(&self) -> Vec<FormatterDefinition>;
}
```

## Performance Budget

| Operation | Target | Strategy |
|-----------|--------|----------|
| Initial index (100K LOC) | < 5s | Parallel parsing, rayon |
| Incremental update (1 file) | < 100ms | Content-hash diffing |
| Impact query (depth=5) | < 200ms | BFS with early termination |
| OmniQL simple query | < 500ms | Index-first scan strategy |
| Memory (100K LOC project) | < 500MB | Arena allocation, mmap |
| Memory (1M LOC project) | < 2GB | Lazy loading, LRU eviction |
