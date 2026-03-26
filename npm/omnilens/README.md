# omnilens

**AI-native code verification engine** — detect semantic changes, invariant violations, and missing tests across Rust, TypeScript, and Python.

```bash
npm install -g omnilens
```

## Quick Start

```bash
cd your-project
omnilens init
omnilens index          # Build semantic graph (42 files in ~110ms)
```

## Commands

### Verify changes (semantic diff)

```bash
omnilens verify --diff HEAD~1          # Compare against last commit
omnilens verify --diff main            # Compare against main branch
omnilens --format json verify --diff HEAD~1   # JSON for CI/CD
```

Output:
```
Semantic Changes (3 total)
  [REVIEW] lib.rs:80 — New function 'ContentHash::is_zero'
  [REVIEW] lib.rs:75 — New function 'ContentHash::from_str_content'
  [BREAKING] auth.rs:42 — 'verify_token': parameter count changed (1 → 2)

PASS 3 semantic changes, 1 warnings | Risk: 19%
```

### Impact analysis

```bash
omnilens impact src/auth.rs --fn verify_token

# Output:
# Who calls this? (3 total)
#   → login (auth.rs:12)
#   → middleware (server.rs:45)
#   →→ main (main.rs:8)
#
# What does it call? (5 total)
#   → jwt.decode (jwt.rs:20)
#   → db.find_user (db.rs:33)
```

### Query the codebase (OmniQL)

```bash
omnilens query "FIND functions WHERE complexity > 15"
omnilens query "FIND functions WHERE calls(db.query) AND visibility = public"
omnilens query "FIND types WHERE fields > 5"
omnilens query "FIND functions WHERE NOT calls(unwrap)"
omnilens query 'FIND functions WHERE name ~ "*test*"'
```

### Discover invariants

```bash
omnilens invariants

# Output:
# INV [CONVENTION] Public functions use snake_case (66/66 = 100%)
# INV [CONVENTION] Public types use PascalCase (76/76 = 100%)
# INV [ERROR-HANDLING] Found 37 functions returning Result
# INV [ORDERING] Initialization function leads to 14 downstream operations
```

## Git Hooks

```bash
omnilens hook install     # Auto-verify on commit and push
omnilens hook uninstall
```

## CI/CD

Works with any platform — auto-detects GitHub, GitLab, Jenkins, Bitbucket, Azure DevOps:

```bash
omnilens ci                        # Auto-detect platform
omnilens --format json ci          # JSON output
omnilens --format sarif ci         # SARIF for code scanning
```

## Output Formats

| Format | Use | Flag |
|--------|-----|------|
| Text | Terminal | `--format text` (default) |
| JSON | CI/CD, AI agents | `--format json` |
| SARIF | GitHub/GitLab Code Scanning | `--format sarif` |

## Supported Languages

| Language | Extensions |
|----------|-----------|
| Rust | `.rs` |
| TypeScript/JavaScript | `.ts` `.tsx` `.js` `.jsx` `.mts` `.mjs` |
| Python | `.py` `.pyi` |

## AI Agent Integration

omnilens can be used as a tool by AI coding agents:

- **MCP Server** — for Claude Desktop/Code
- **Python SDK** — `pip install omnilens` with LangChain + OpenAI function calling
- **OpenAPI spec** — REST API for any agent
- **JSON output** — parseable by any system

See [AI Integration Guide](https://github.com/injaehwang/omnilens/blob/main/docs/ai-integration.md).

## Links

- [GitHub](https://github.com/injaehwang/omnilens)
- [Releases](https://github.com/injaehwang/omnilens/releases)
- [Documentation](https://github.com/injaehwang/omnilens/blob/main/docs/)

## License

Apache-2.0 OR MIT
