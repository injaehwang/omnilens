# omnilens

**Your AI's eyes into your codebase.**

omnilens analyzes your project in milliseconds and gives AI a complete map of your code. AI uses this to understand, modify, and verify your code without breaking things.

## Install

```bash
npm install -g omnilens
```

## Use

### Step 1: Analyze your project

```bash
cd your-project
omnilens
```

Output:
```
  omnilens 11ms | 45 files | 320 functions | 87 types
  Health: 85/100
  Cross-file deps: 142

  Tell your AI: "let's start omnilens"
```

### Step 2: Tell your AI to start

Open your AI tool and say:

| AI tool | What to type |
|---------|-------------|
| Claude Code | `let's start omnilens` |
| Cursor | `let's start omnilens` |
| Gemini | `let's start omnilens` |
| ChatGPT | `let's start omnilens` |
| Windsurf | `let's start omnilens` |
| Any AI | `let's start omnilens` |

Any variation works: `omnilens`, `start omnilens`, `omnilens 시작`, `review omnilens snapshot` — anything mentioning "omnilens".

AI reads the analysis and responds:

> "Project analyzed. 45 files, 320 functions. What would you like to do?"

### Step 3: Work with your AI

Just tell it what you need. AI uses omnilens internally to verify its work.

```
You: "Add empty state handling to all tables"
You: "Fix the login function — it's not handling errors"
You: "Refactor auth service into smaller functions"
```

AI modifies your code, checks for breaking changes, and fixes them automatically.

## How it works

1. `omnilens` creates `.omnilens/snapshot.json` — a complete map of your project
2. AI reads the snapshot and understands every file, function, and dependency
3. When AI modifies code, it runs `omnilens verify` to catch errors
4. If something breaks, AI fixes it before telling you it's done

## Supported languages

Rust · TypeScript · JavaScript · Python

## Performance

| Project size | Time |
|-------------|------|
| 10 files | ~10ms |
| 100 files | ~100ms |
| 1000 files | ~1s |

## License

Apache-2.0 OR MIT
