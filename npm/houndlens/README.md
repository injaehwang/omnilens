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

### 2. Unified verification

`houndlens verify` runs your project's own tools on changed files and combines the results:

```
houndlens verify --diff HEAD

  [houndlens] syntax: 0 errors
  [tsc] type check: 1 error — auth.ts:42 missing argument
  [eslint] lint: 0 warnings
  
  1 error total
```

One command. All tools. Only changed files.

### 3. AI feedback loop

After AI modifies code:

```
AI modifies auth.ts
  → houndlens (rescan, 10ms)
  → houndlens verify (runs tsc + eslint on auth.ts only)
  → AI reads result: "auth.ts:42 missing argument"
  → AI fixes auth.ts
  → houndlens verify: 0 errors
  → done
```

### 4. Commit protection

Git pre-commit hook blocks broken code. Works with all AI tools.

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
