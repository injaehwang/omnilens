# omnilens

**Your AI's eyes into your codebase.**

omnilens analyzes your entire project in milliseconds and gives AI a complete map — every file, function, type, call graph, and dependency. AI uses this to understand your code and work on it.

## Install

```bash
npm install -g omnilens
```

## Use

```bash
cd your-project
omnilens
```

That's it. Output:

```
  omnilens 11ms | 13 files | 45 functions | 9 types
  Health: 100/100
  Cross-file deps: 23

  .omnilens/snapshot.json
```

Now open your AI (Claude, Cursor, GPT, whatever) and start working. AI reads `snapshot.json` and understands your project instantly.

## What AI gets

`snapshot.json` contains:

- **Every file** with functions, types, imports
- **Call graph** — who calls what, across files
- **Complexity scores** — which functions are risky
- **Cross-file dependencies** — change X, Y breaks
- **Health score** — overall project quality
- **Hotspots** — where bugs are likely hiding

AI doesn't need to open files one by one. It gets the full picture in one read.

## What AI can do with omnilens

When AI needs deeper analysis, it calls omnilens internally:

```bash
# "What breaks if I change this function?"
omnilens impact src/auth.ts --fn login

# "Did my changes break anything?"
omnilens verify --format json --diff HEAD~1

# "Find all complex functions"
omnilens query "FIND functions WHERE complexity > 15"

# "Generate tests for untested code"
omnilens fix
```

You don't run these. AI does.

## Supported languages

Rust, TypeScript, JavaScript, Python

## Performance

| Project size | Analysis time |
|-------------|---------------|
| 10 files | ~10ms |
| 100 files | ~100ms |
| 1000 files | ~1s |

## License

Apache-2.0 OR MIT
