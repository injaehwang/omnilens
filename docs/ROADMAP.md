# omnilens Roadmap

## Phase 1 — MVP (Month 1-3)

**Goal**: `omnilens impact` working for Rust/TS/Python, demo-ready for launch.

### Milestones

- [ ] **M1.1** Workspace setup, CI/CD, Cargo workspace structure
- [ ] **M1.2** tree-sitter integration + Rust/TS/Python frontends (parsing only)
- [ ] **M1.3** USIR v0.1 — functions, calls, imports (no data flow yet)
- [ ] **M1.4** Semantic graph with basic forward/reverse impact traversal
- [ ] **M1.5** Content-addressed storage + incremental indexing
- [ ] **M1.6** CLI: `omnilens init`, `omnilens index`, `omnilens impact`
- [ ] **M1.7** Output formats: terminal (colored), JSON, SARIF
- [ ] **M1.8** Launch: HN post, Reddit r/rust + r/programming, Twitter/X

### Deliverables
- Single binary, `cargo install omnilens`
- Works on any Rust/TS/Python project with zero configuration
- `omnilens impact <file> --fn <name>` returns direct + transitive callers

---

## Phase 2 — Core Intelligence (Month 4-6)

**Goal**: OmniQL, data flow analysis, 5+ language support.

### Milestones

- [ ] **M2.1** OmniQL parser + execution engine
- [ ] **M2.2** Data flow analysis (taint tracking: UserInput → DB → Response)
- [ ] **M2.3** Go + Java frontends
- [ ] **M2.4** Type resolution across modules (best-effort for dynamic languages)
- [ ] **M2.5** API endpoint detection (HTTP routes, gRPC, GraphQL)
- [ ] **M2.6** LSP server: `omnilens serve`
- [ ] **M2.7** VS Code extension (basic: impact on hover, query palette)

### Deliverables
- `omnilens query "..."` works across all supported languages
- Security-focused queries (injection, XSS, SSRF patterns)
- VS Code extension on marketplace

---

## Phase 3 — Runtime + Test Gen (Month 7-9)

**Goal**: Runtime profiling overlay, intelligent test generation.

### Milestones

- [ ] **M3.1** eBPF-based runtime tracer (Linux)
- [ ] **M3.2** Runtime overlay on semantic graph (call frequency, timing)
- [ ] **M3.3** ETW backend (Windows) + DTrace backend (macOS)
- [ ] **M3.4** Constraint-based test generation (boundary + error paths)
- [ ] **M3.5** Test output in native framework format (pytest, jest, cargo test)
- [ ] **M3.6** WASM plugin system v1

### Deliverables
- `omnilens trace --attach <pid>` with live analysis
- `omnilens testgen` generates runnable tests
- Plugin API for community extensions

---

## Phase 4 — Ecosystem (Month 10-12)

**Goal**: Cross-repo intelligence, CI integration, cloud dashboard.

### Milestones

- [ ] **M4.1** Cross-repository dependency graph
- [ ] **M4.2** Breaking change detection across repos
- [ ] **M4.3** CI/CD integration (GitHub Actions, GitLab CI)
- [ ] **M4.4** `omnilens-cloud` — Go-based API server + dashboard
- [ ] **M4.5** Team collaboration: shared analysis, PR impact reviews
- [ ] **M4.6** C/C++ frontend
- [ ] **M4.7** JetBrains plugin

### Deliverables
- `omnilens xrepo` for organization-wide analysis
- GitHub Action: auto-comment PR impact analysis
- Web dashboard for team-wide code intelligence

---

## Beyond v1.0

- Natural language queries ("find all functions that could cause a deadlock")
- AI-assisted refactoring suggestions based on impact analysis
- Historical trend analysis (complexity growth, dependency health)
- Compliance checking (GDPR data flow, PCI-DSS boundaries)
- Custom rule engines for organization-specific patterns
