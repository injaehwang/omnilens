# houndlens

**Give your AI the full picture.**

houndlens analyzes your project in milliseconds and gives AI everything it needs — structure, dependencies, and verified results from your project's own tools.

## Install

```bash
npm install -g houndlens
```

## Use

```bash
cd your-project
houndlens                    # analyze project (10ms)
```

Then tell your AI: `let's start houndlens`

## What it does

### 1. Instant project analysis

houndlens parses every file, builds a call graph, and outputs a lightweight summary for AI.

```
.houndlens/summary.json (3KB) — project structure, dependencies, health
.houndlens/changes.json       — what changed since last scan
```

AI reads these instead of opening files one by one.

### 2. AI runs all checks for you

When AI modifies code, it runs `houndlens verify` which executes your project's own tools on changed files:

```
houndlens verify --diff HEAD

  [houndlens] syntax: 0 errors
  [tsc] type check: 1 error — auth.ts:42 missing argument
  [eslint] lint: 0 warnings
  
  1 error total
```

One command. All tools. Only changed files.

### 3. AI ↔ houndlens verification loop

**Claude Code** — automatic ping-pong:
```
AI modifies auth.ts
  → hook fires automatically
  → houndlens verify runs (tsc + eslint on auth.ts only)
  → result injected into AI's conversation: "auth.ts:42 missing argument"
  → AI fixes auth.ts
  → hook fires again → 0 errors
  → done
```
AI doesn't choose to verify — it's forced by the hook.

**Other AI tools** (Cursor, Gemini, Codex, etc.) — instruction-based:
```
AI modifies auth.ts
  → AI follows ai-instructions.md
  → runs houndlens verify
  → reads result, fixes errors
```
Works when AI follows instructions. Not guaranteed.

### 4. Commit protection

**All AI tools** — git pre-commit hook blocks broken code. No exceptions. Even if AI skipped verification, broken code cannot be committed.

## Supported languages

Rust · TypeScript · JavaScript · Python

## Performance

| Project | Analysis |
|---------|----------|
| 10 files | ~10ms |
| 100 files | ~100ms |
| 1000 files | ~1s |

Verification time depends on your project's tools (tsc, eslint, etc.)

## License

Apache-2.0 OR MIT
