# HoundLens

**Give your AI the full picture.**

houndlens analyzes your project in milliseconds and gives AI everything it needs — structure, dependencies, and verified results from your project's own tools.

## Install

```bash
npm install -g houndlens
```

Or from source:
```bash
cargo install --git https://github.com/injaehwang/houndlens
```

## Use

```bash
cd your-project
houndlens                    # analyze project (~10ms)
```

Then tell your AI: `let's start houndlens`

## What it does

### 1. Instant project analysis

houndlens parses every file, builds a call graph, and outputs a lightweight summary for AI.

```
.houndlens/summary.json — file map, function signatures, dependencies, health
.houndlens/changes.json — what changed since last scan
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

**Claude Code** — automatic ping-pong (hook-enforced):
```
AI modifies auth.ts
  → hook fires → houndlens verify → result injected into AI conversation
  → AI fixes → hook fires again → 0 errors → done
```

**Other AI tools** — instruction-based (not guaranteed):
```
AI modifies auth.ts
  → AI follows ai-instructions.md → runs houndlens verify → fixes errors
```

### 4. Commit protection

**All AI tools** — git pre-commit hook blocks broken code. No exceptions.

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
