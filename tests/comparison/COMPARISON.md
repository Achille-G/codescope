# Token Usage Comparison: Classical Search vs Codescope

This document compares the token usage when AI agents explore code using
classical search methods (grep, find, cat) versus codescope semantic search.

## Test Scenario

**Task**: Find where user authentication is handled and understand the login flow.

**Repository**: `fake-project/` (a small TypeScript project)
- 4 source files
- ~400 lines of code

## Classical Search Approach

An AI agent without codescope might do the following:

```bash
# Step 1: Find all TypeScript files
find . -name "*.ts"

# Output: 4 files found
# Tokens: ~50 (command) + ~100 (output) = 150 tokens

# Step 2: Search for "login"
grep -rn "login" --include="*.ts" .

# Output: Multiple matches across files
# Tokens: ~60 (command) + ~500 (output with context) = 560 tokens

# Step 3: Read the file that looks most relevant
cat src/services/auth_service.ts

# Output: Entire file content
# Tokens: ~30 (command) + ~2000 (file content) = 2030 tokens

# Step 4: Understand related models
cat src/models/user.ts

# Tokens: ~30 (command) + ~1200 (file content) = 1230 tokens

# Step 5: Check database utilities
cat src/utils/database.ts

# Tokens: ~30 (command) + ~1500 (file content) = 1530 tokens
```

**Total Classical Approach: ~5,500 tokens**

## Codescope Semantic Search Approach

With codescope indexed:

```bash
# Step 1: Semantic search for authentication
codescope search "user authentication login flow"

# Output: Top 3 relevant chunks with snippets
# - AuthService.login (auth_service.ts:41-62)
# - AuthService.logout (auth_service.ts:68-75)
# - AuthService.refreshToken (auth_service.ts:77-92)

# Tokens: ~80 (command) + ~600 (JSONL output with snippets) = 680 tokens
```

**Total Codescope Approach: ~680 tokens**

## Token Savings

| Approach | Tokens Used | Reduction |
|----------|-------------|-----------|
| Classical | ~5,500 | - |
| Codescope | ~680 | **88%** |

## Why Codescope is More Efficient

1. **Semantic Understanding**: Codescope finds relevant code chunks based on
   meaning, not just keyword matching. A search for "authentication" finds
   the `login` function even if it doesn't contain that exact word.

2. **Chunked Output**: Instead of returning entire files, codescope returns
   only the relevant functions/classes with contextual snippets.

3. **Ranked Results**: Results are ranked by relevance, so the agent can
   focus on the most important code first.

4. **Hybrid Search**: Combines BM25 keyword matching with vector similarity
   for better recall than either alone.

## Real-World Impact

For a typical development task requiring 10 search iterations:
- Classical: 10 × 5,500 = 55,000 tokens
- Codescope: 10 × 680 = 6,800 tokens

This represents an **88% reduction** in context window usage, allowing:
- Longer conversations without hitting limits
- More complex tasks in a single session
- Lower API costs for teams using AI assistants

## Reproducing This Comparison

```bash
# Initialize codescope in the fake-project
cd tests/comparison/fake-project
codescope init
codescope index

# Search with codescope
codescope search "user authentication login" --type semantic

# Compare with grep
grep -rn "login" --include="*.ts" .
```

## Notes

- Token counts are approximate and depend on the tokenizer
- Real-world savings may vary based on codebase size and query complexity
- Larger codebases show even greater token savings with codescope
