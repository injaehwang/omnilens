<p align="center">
  <img src="docs/assets/logo-placeholder.svg" alt="omnilens" width="200" />
</p>

<h1 align="center">omnilens</h1>

<p align="center">
  <strong>Universal Codebase Intelligence Engine</strong><br/>
  Understand, analyze, and predict the impact of every change across any language.
</p>

<p align="center">
  <a href="#installation">Installation</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#features">Features</a> •
  <a href="#architecture">Architecture</a> •
  <a href="docs/CONTRIBUTING.md">Contributing</a>
</p>

---

## What is omnilens?

omnilens is a **language-agnostic code intelligence engine** that builds a semantic understanding of your entire codebase. It combines static analysis, data flow tracking, and runtime profiling to answer the questions developers ask every day:

- **"If I change this function, what breaks?"** → `omnilens impact`
- **"Where is user input used without sanitization?"** → `omnilens query`
- **"Why is this endpoint slow?"** → `omnilens trace`
- **"What tests am I missing?"** → `omnilens testgen`

Unlike traditional linters or LSP servers that work at the syntax level, omnilens operates on a **Universal Semantic IR** — a language-independent intermediate representation that captures meaning, not just structure.

## Installation

```bash
# From source (requires Rust 1.75+)
cargo install omnilens

# Or download prebuilt binary
curl -fsSL https://omnilens.dev/install.sh | sh
```

## Quick Start

```bash
# Initialize omnilens in your project (auto-detects languages)
omnilens init

# Analyze impact of a change
omnilens impact src/auth/token.rs --fn verify

# Query across all languages
omnilens query "functions that read from database without error handling"

# Live runtime tracing
omnilens trace --attach pid:4521

# Generate missing tests
omnilens testgen src/payment/checkout.rs --strategy boundary
```

## Features

### 🔍 Impact Prediction Engine
Predict the full blast radius of any code change — direct callers, transitive dependencies, affected API endpoints, and test coverage gaps.

### 🌐 Universal Semantic Graph
Query your codebase semantically across all languages using OmniQL. Find patterns, anti-patterns, and security vulnerabilities regardless of implementation language.

### ⚡ Runtime-Aware Analysis
Overlay runtime profiling data (via eBPF/ETW/DTrace) onto static analysis graphs. See how code actually executes, not just how it's written.

### 🧪 Smart Test Generation
Generate tests targeting uncovered critical paths using symbolic execution guided by runtime data.

### 🔗 Cross-Repository Intelligence
Track dependencies across repositories. Understand who consumes your APIs and how breaking changes propagate.

## Architecture

See [docs/architecture.md](docs/architecture.md) for the full technical deep-dive.

```
┌──────────────────────────────────────────────────────┐
│                  CLI / IDE Plugin (LSP)               │
├──────────────────────────────────────────────────────┤
│                 Query Engine (OmniQL)                 │
├────────┬──────────┬───────────┬───────────────────── ┤
│Semantic│Data Flow │ Runtime   │ Cross-Repo            │
│Graph   │Analyzer  │ Profiler  │ Intelligence          │
├────────┴──────────┴───────────┴──────────────────────┤
│           Universal Semantic IR (USIR)                │
├──────────────────────────────────────────────────────┤
│    Language Frontends (tree-sitter based parsers)     │
├──────────────────────────────────────────────────────┤
│     Incremental Indexing Engine (content-addressed)   │
└──────────────────────────────────────────────────────┘
```

## Supported Languages

| Language | Parsing | Semantic IR | Data Flow | Runtime Trace |
|----------|---------|-------------|-----------|---------------|
| Rust     | ✅      | ✅          | ✅        | 🔜            |
| TypeScript/JavaScript | ✅ | ✅   | ✅        | 🔜            |
| Python   | ✅      | ✅          | ✅        | 🔜            |
| Go       | ✅      | 🔜          | 🔜        | 🔜            |
| Java     | 🔜      | 🔜          | 🔜        | 🔜            |
| C/C++    | 🔜      | 🔜          | 🔜        | 🔜            |

## License

Apache-2.0 OR MIT — your choice.
