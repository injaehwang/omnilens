# omnilens — Vision

## The Problem: AI Generates Code Faster Than Humans Can Verify

The developer workflow has fundamentally changed:

```
Before AI:  Human writes code → Human writes tests → Human reviews
Now:        AI generates code → ??? → Ship it?
```

AI coding assistants (Copilot, Claude, Cursor, Devin) produce code at 10-100x human speed.
But the **verification pipeline** hasn't evolved:

| Stage | Speed Before AI | Speed After AI | Gap |
|-------|----------------|----------------|-----|
| Code generation | Hours | Seconds | ✅ Solved |
| Code review | Hours | Hours | ❌ Bottleneck |
| Test writing | Hours | Hours | ❌ Bottleneck |
| Test execution | Minutes | Minutes | ⚠️ Unchanged |
| Impact analysis | Days | Days | ❌ Bottleneck |
| Bug detection | Days-Weeks | Days-Weeks | ❌ Bottleneck |

**The bottleneck has shifted from writing code to verifying code.**

### AI-Generated Code Has Unique Failure Modes

Traditional bugs are usually **local** — a typo, off-by-one, null pointer.
AI-generated bugs are **systemic** and harder to catch:

1. **Plausible but wrong**: Code looks correct, passes superficial review, but has subtle logic errors
2. **Hallucinated APIs**: Uses functions/methods that don't exist or have wrong signatures
3. **Context mismatch**: Correct code for wrong context (wrong DB schema, wrong auth model)
4. **Semantic drift**: Each AI-generated change is locally correct but globally inconsistent
5. **Stale patterns**: AI trained on old patterns that conflict with current codebase conventions
6. **Hidden coupling**: AI doesn't understand runtime behavior, creates invisible dependencies

**No existing tool catches these systematically.**

## The Solution: omnilens — AI-Native Code Verification Engine

omnilens is not a testing *framework*. It's a **verification engine** designed for the AI-augmented development workflow.

### Core Thesis

> If AI can generate 100 lines of code per minute, the verification system must analyze 100 lines per minute — with deeper understanding than a human reviewer.

### What "AI-Native Testing" Means

| Traditional Testing | omnilens (AI-Native) |
|---------------------|----------------------|
| Human writes test cases | Engine **generates** verification automatically |
| Tests check known behaviors | Engine discovers **unknown behaviors** |
| Unit/integration/e2e categories | **Semantic verification** across all levels |
| Test after code is written | **Verify as code is generated** (real-time) |
| Coverage = lines executed | Coverage = **behaviors verified** |
| Pass/fail binary | **Confidence score** with evidence |
| Manual impact analysis | **Automatic blast radius** prediction |
| Assumes code is human-written | **Assumes code may be AI-generated** (different error model) |

### The Five Pillars of AI-Native Verification

#### Pillar 1: Semantic Diff Analysis
Not "what lines changed" but "what behaviors changed."

```bash
$ omnilens verify --diff HEAD~1

Semantic Changes Detected:
  ✗ auth/login.rs: Return type changed from Result<Token> to Option<Token>
    → 12 callers expect Result, will fail silently with None
    → AI likely hallucinated: Option doesn't carry error context
    Confidence: HIGH RISK

  ✓ api/users.rs: Added pagination to list endpoint
    → Backward compatible, existing callers unaffected
    → Test generated: verify default page_size matches API spec
    Confidence: SAFE
```

#### Pillar 2: Invariant Discovery & Enforcement
Automatically discover what's **always true** in the codebase, then verify AI-generated code doesn't violate it.

```bash
$ omnilens invariants

Discovered Invariants:
  INV-001: All DB queries go through connection pool (never direct connect)
  INV-002: User-facing errors never expose internal stack traces
  INV-003: All API endpoints require authentication middleware
  INV-004: Monetary values use Decimal, never f64

$ omnilens verify src/ai-generated-payment.rs

  ✗ VIOLATION INV-004: Line 23 uses f64 for `total_amount`
    → AI-generated code used f64 instead of Decimal
    → This causes rounding errors in financial calculations
    → Auto-fix available: replace f64 with rust_decimal::Decimal
```

#### Pillar 3: Behavioral Contract Testing
Verify that AI-generated code honors the **implicit contracts** of the codebase.

```bash
$ omnilens contracts src/auth/

Inferred Contracts:
  fn verify_token(token: &str) -> Result<Claims>
    PRE:  token.len() > 0
    PRE:  token matches JWT format (xxx.xxx.xxx)
    POST: Ok(claims) → claims.exp > now()
    POST: Err(_) → no side effects (pure function)
    INVARIANT: called before any protected resource access

$ omnilens verify src/ai-generated-middleware.rs

  ✗ CONTRACT VIOLATION: verify_token() called AFTER resource access on line 45
    → AI placed auth check after database query (should be before)
```

#### Pillar 4: Continuous Verification Loop
Real-time verification as AI generates code, not after.

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│ AI generates │────▶│ omnilens     │────▶│ Feedback to  │
│ code chunk   │     │ verifies     │     │ AI / Human   │
└─────────────┘     │ in real-time │     └─────────────┘
                    └──────────────┘
                          │
                    ┌─────▼──────┐
                    │ Updates    │
                    │ semantic   │
                    │ graph      │
                    └────────────┘
```

Integration points:
- **LSP**: Verify on save, show inline diagnostics
- **CI/CD**: Gate PRs with semantic verification
- **AI Agent SDK**: Provide verification API for AI coding agents
- **Git hooks**: Pre-commit semantic verification

#### Pillar 5: Property-Based Test Synthesis
Generate tests that verify **properties**, not examples.

```bash
$ omnilens testgen src/cart.rs --mode property

Generated Property Tests:
  prop_cart_total_is_sum_of_items:
    ∀ items: Vec<Item>, cart.add_all(items) → cart.total() == items.sum(price * qty)

  prop_cart_remove_decreases_total:
    ∀ cart with items, cart.remove(item) → cart.total() < old_total

  prop_cart_empty_after_clear:
    ∀ cart, cart.clear() → cart.items().is_empty() && cart.total() == 0

  prop_cart_concurrent_safe:
    ∀ ops: Vec<CartOp>, parallel_apply(cart, ops) → cart.is_consistent()
```

## Target Users

### Primary: Teams Using AI Coding Assistants
- Using Copilot/Claude/Cursor daily
- Shipping AI-generated code without adequate verification
- Need automated safety net that scales with AI output speed

### Secondary: AI Agent Builders
- Building autonomous coding agents (Devin-like systems)
- Need programmatic verification API
- Want to create self-correcting agent loops

### Tertiary: Security-Conscious Teams
- Need to verify AI-generated code doesn't introduce vulnerabilities
- Compliance requirements for AI-generated code auditing
- Want provable correctness guarantees

## Competitive Landscape

| Tool | What It Does | What It Doesn't Do |
|------|-------------|-------------------|
| **Linters** (eslint, clippy) | Syntax/style checks | Semantic understanding, cross-file analysis |
| **Type checkers** (tsc, mypy) | Type correctness | Behavioral correctness, invariants |
| **AI review** (CodeRabbit) | LLM-based review | Formal verification, runtime awareness |
| **Fuzzing** (AFL, cargo-fuzz) | Input mutation testing | Semantic targeting, property synthesis |
| **Formal verification** (Dafny, KLEE) | Mathematical proofs | Practical for real codebases, multi-language |
| **omnilens** | **All of the above, unified** | — |

## Success Metrics

- **Time to verify**: AI generates a 500-line PR → omnilens verifies in < 30 seconds
- **Bug detection rate**: Catch > 80% of AI-specific bug patterns before merge
- **False positive rate**: < 5% (unusable if noisy)
- **Language coverage**: Top 5 languages from day one
- **Zero config**: Works on any project with `omnilens init`

## Why This Wins GitHub Stars

1. **Perfect timing**: Every developer is using AI coding tools NOW
2. **Universal pain**: Everyone worries about AI code quality but has no systematic solution
3. **"Holy shit" demo**: `omnilens verify --diff HEAD~1` on an AI-generated PR → instant, deep analysis
4. **Technical moat**: Combining formal methods + semantic analysis + runtime data is genuinely hard
5. **Daily use tool**: Not a one-time setup — used on every PR, every commit, every AI interaction
