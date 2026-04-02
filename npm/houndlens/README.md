# houndlens

**AI guard for your codebase.** Give your AI the full picture. Save your tokens.

houndlens analyzes your entire project in milliseconds and wraps your AI with guardrails — so it understands your code, doesn't break things, and verifies its own work.

## Install

```bash
npm install -g houndlens
```

## Use

### Step 1: Analyze your project

```bash
cd your-project
houndlens
```

Output:
```
  houndlens 11ms | 45 files | 320 functions | 87 types
  Health: 85/100
  Cross-file deps: 142

  Tell your AI: "let's start houndlens"
```

### Step 2: Tell your AI to start

Open your AI tool and say:

| AI tool | What to type |
|---------|-------------|
| Claude Code | `let's start houndlens` |
| Cursor | `let's start houndlens` |
| Gemini | `let's start houndlens` |
| ChatGPT | `let's start houndlens` |
| Windsurf | `let's start houndlens` |
| Any AI | `let's start houndlens` |

Any variation works — `houndlens`, `start houndlens`, `houndlens 시작` — anything mentioning "houndlens".

### Step 3: Work with your AI

Just tell it what you need. houndlens works behind the scenes.

## How it works

houndlens wraps your AI's coding workflow with automated analysis and verification.

```
Developer: "let's start houndlens"
        ↓
   AI ← reads .houndlens/summary.json (3KB)
   AI: "Project analyzed. What do you need?"
        ↓
Developer: "Fix the login function"
        ↓
   AI → modifies auth.ts
   AI → runs: houndlens (rescan, 10ms)
   AI ← reads .houndlens/changes.json
        "login signature changed → handleLogin affected"
   AI → modifies api.ts (fixes handleLogin)
   AI → runs: houndlens verify --diff HEAD
   AI ← reads result: 0 errors
   AI: "Done. Fixed auth.ts and api.ts."
        ↓
Developer: git commit
        ↓
   houndlens pre-commit hook → verify → pass → committed
```

AI and houndlens work in a loop: AI modifies → houndlens checks → AI reads result → AI fixes → repeat until clean.


### What houndlens generates

```
.houndlens/
  snapshot.json        Full project analysis (internal use)
  summary.json         Lightweight overview for AI (3KB)
  changes.json         What changed since last scan
  ai-instructions.md   How AI should behave

.git/hooks/pre-commit  Blocks broken commits (all AI tools)
.claude/hooks/         Real-time verify for Claude Code
CLAUDE.md              One-line pointer for Claude
.cursorrules           One-line pointer for Cursor
.windsurfrules         One-line pointer for Windsurf
```

### Three layers of protection

| Layer | Scope | How |
|-------|-------|-----|
| **AI instructions** | All AI tools | summary.json tells AI to run `houndlens` and read `changes.json` after modifications |
| **Claude Code hooks** | Claude Code only | PostToolUse hook auto-runs verify and sends results to AI's conversation context |
| **Git pre-commit hook** | All AI tools | Blocks commit if breaking changes exist — last line of defense |

### Token efficiency

| Without houndlens | With houndlens |
|-------------------|----------------|
| AI opens files one by one to understand structure | AI reads summary.json (~3KB) |
| AI guesses what depends on what | AI knows the call graph |
| AI re-reads files after every change | AI reads changes.json (~500B) |

Summary.json stays small regardless of project size — it contains structure, not source code.

## Supported languages

Rust · TypeScript · JavaScript · Python

## Performance

| Project size | Analysis time |
|-------------|---------------|
| 10 files | ~10ms |
| 100 files | ~100ms |
| 1000 files | ~1s |

## License

Apache-2.0 OR MIT
