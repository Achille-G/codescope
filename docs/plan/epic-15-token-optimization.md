# Epic 15: Token Optimization for AI Agents

**Status**: 🟡 In Progress

## Description

Reduce token consumption in AI agent workflows by implementing:
- Automatic deduplication of overlapping chunks
- Compact mode (file:line references only)
- Configurable excerpt length

**Target**: 70-95% token savings on search results for agent use cases.

## Motivation

Current search results return full code excerpts, causing high token usage:
- Overlapping chunks waste tokens (auth.js:1-50, auth.js:41-90)
- Full code snippets exceed agent context windows
- AI agents only need file references + relevant lines

## Tickets

### 15.1 Automatic Chunk Deduplication

**Status**: 🟢 Done

**Problem**: Overlapping chunks (>50% overlap) are returned as separate results.

**Before**:
```
auth.js:1-50
auth.js:41-90    ← 40 lines overlap
auth.js:81-130   ← 40 lines overlap
```

**After**:
```
auth.js:1-50     ← Only one chunk retained
```

**Implementation**:
- Add overlap detection in `ResultFormatter`
- Keep first chunk, discard overlapping subsequent chunks
- Threshold: 50% overlap triggers deduplication

**Files**:
- `crates/codescope-search/src/dedupe.rs` (new)
- `crates/codescope-search/src/result.rs`

**Tasks**:
- [ ] Create `ChunkDeduplicator` struct
- [ ] Implement overlap calculation (lines shared / total)
- [ ] Add threshold config (default: 0.5)
- [ ] Wire into result formatting pipeline
- [ ] Add `--dedupe` flag (default: true)

**Savings**: ~70% fewer tokens on typical queries

---

### 15.2 Compact Mode (--compact)

**Status**: 🟢 Done

**Description**: Returns only file:line references without code excerpts.

**Output format**:
```
/path/to/auth.js:78-128 (Score: 0.42, javascript)
/path/to/adminAuth.js:16-66 (Score: 0.39, javascript)
/path/to/billingAuth.js:63-113 (Score: 0.38, javascript)
```

**Implementation**:
- Add `compact: bool` field to `SearchResult`
- Create separate serialization for compact output
- Add `--compact` CLI flag

**Files**:
- `crates/codescope-search/src/result.rs`
- `crates/codescope-cli/src/commands/search.rs`

**Tasks**:
- [ ] Add `compact` field to `SearchResult`
- [ ] Create compact serialization format
- [ ] Add `--compact` flag to search command
- [ ] Test with AI agent workflow
- [ ] Document in CLI help

**Savings**: ~95% fewer tokens vs full excerpts

---

### 15.3 Configurable Excerpt Lines (--excerpt-lines)

**Status**: ⚪ Not Started

**Description**: Limit displayed lines per chunk to N lines.

**Usage**:
```bash
codescope search "auth middleware" --excerpt-lines 15
```

**Output**:
```javascript
function authenticateJWT(req, res, next) {
  const token = req.headers.authorization;
  if (!token) {
    return res.status(401).json({ error: 'No token' });
  }
  // ... 10 more lines hidden
}
```

**Implementation**:
- Add `excerpt_lines: Option<usize>` to config
- Truncate snippet in result formatting
- Preserve full data for later Read tool use

**Files**:
- `crates/codescope-search/src/result.rs`
- `crates/codescope-cli/src/commands/search.rs`

**Tasks**:
- [ ] Add `excerpt_lines` config field
- [ ] Implement snippet truncation logic
- [ ] Add `--excerpt-lines <N>` flag
- [ ] Handle edge case: N > chunk size
- [ ] Add validation (N >= 1)

**Savings**: ~70% fewer tokens (15 lines vs 50+ lines)

---

### 15.4 Unified Token-Saving Flags

**Status**: ⚪ Not Started

**Description**: Combine flags for optimal agent workflows.

**Proposed Flags**:
```bash
# Ultra-economical: just file references
codescope search "query" --compact

# Economical: short excerpts
codescope search "query" --excerpt-lines 10

# Standard: full excerpts
codescope search "query" --no-compact

# Combination: compact + limit
codescope search "query" --compact -n 10
```

**Mode Examples**:

| Mode | Flags | Tokens | Use Case |
|------|-------|--------|----------|
| Discovery | `--compact` | ~500 | Where is the code? |
| Overview | `--excerpt-lines 15` | ~2,000 | How does it work? |
| Analysis | (default) | ~8,000 | Deep analysis |

**Files**:
- `crates/codescope-cli/src/commands/search.rs`
- `crates/codescope-core/src/config.rs`

**Tasks**:
- [ ] Define enum `OutputMode { Full, Compact, Truncated }`
- [ ] Add `--compact` and `--excerpt-lines` flags
- [ ] Add `--no-dedupe` flag for debugging
- [ ] Update CLI documentation
- [ ] Add integration tests for each mode

---

### 15.5 Agent Workflow Documentation

**Status**: ⚪ Not Started

**Description**: Document optimal workflows for AI agents.

**Recommended Workflow**:

```
Step 1: Compact search
$ codescope search "error handling" --compact -n 10
→ ~500 tokens

Step 2: Identify relevant files from results
→ Claude identifies: api/errors.js, utils/logger.ts

Step 3: Read full files with Read tool
→ ~1,500 tokens per file

Step 4: Synthesize answer
→ Minimal additional tokens

Total: ~3,500 tokens vs ~15,000 tokens (full excerpts)
```

**Files**:
- `crates/codescope-cli/src/commands/agent_setup.rs`
- Update AGENTS.md, CLAUDE.md templates

**Tasks**:
- [ ] Update `agent_setup.rs` with new flags
- [ ] Add agent workflow examples
- [ ] Document token savings calculations
- [ ] Add --compact and --excerpt-lines to usage examples

---

## Dependencies

- Epic 6 (Search Engine) - Base search functionality
- Epic 7 (CLI) - CLI argument parsing

## Deliverables

- [ ] Chunk deduplication (>50% overlap removal)
- [ ] `--compact` flag (file:line only)
- [ ] `--excerpt-lines <N>` flag
- [ ] `--no-dedupe` flag for debugging
- [ ] Updated agent setup documentation
- [ ] Integration tests for all modes
- [ ] Target: 70-95% token savings on agent queries

## Related Issues

- Closes #15 (Token optimization for agents)
- Relates to #13 (Agent setup command)
