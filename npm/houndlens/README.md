# houndlens

**AI harness for your codebase.** Give your AI the full picture. Save your tokens.

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

houndlens is an **AI harness** — it wraps your AI's coding workflow with automated analysis and verification.

```
You say "houndlens" in AI chat
        ↓
AI reads .houndlens/summary.json (3KB, not the full snapshot)
AI understands your entire project: files, functions, dependencies, health
        ↓
You tell AI what to do
        ↓
AI modifies code
        ↓
houndlens harness kicks in:
  · Git pre-commit hook blocks broken code (all AI tools)
  · Claude Code hook sends verify results to AI's context
  · AI reads .houndlens/changes.json to see what it broke
  · AI fixes issues automatically
        ↓
Clean code committed
```

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
| **Instructions** | All AI tools | AI reads rules from summary.json |
| **Claude hooks** | Claude Code | Verify results injected into AI context after every edit |
| **Git hook** | All AI tools | Pre-commit blocks broken code — nothing escapes |

### Token efficiency

| Without houndlens | With houndlens |
|-------------------|----------------|
| AI opens files one by one | AI reads 3KB summary |
| AI guesses dependencies | AI knows the call graph |
| AI hopes nothing broke | AI verifies with changes.json |
| ~50,000 tokens to understand project | ~1,000 tokens |

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
