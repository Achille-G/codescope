# CLI Reference

`codescope` is a per-repo CLI. It stores all per-project state in a `.codescope/` directory at the project root.

## Common options

- `-v, --verbose`: more logs (debug for codescope crates)
- `-q, --quiet`: errors only

## Help

Every subcommand has its own help output (you can drill down as deep as you want):

```bash
codescope --help
codescope search --help
codescope trace --help
codescope trace callees --help
```

## `codescope init`

Initializes `.codescope/` in the current directory:
- creates `.codescope/config.toml`
- creates `.codescope/meta.sqlite` (SQLite metadata DB)
- creates `.codescope/tantivy/` (BM25 index directory)

Options:
- `-p, --profile <light|default|heavy>`: resource profile used by indexing/embedding defaults
- `-f, --force`: re-initialize if `.codescope/` already exists

Examples:

```bash
codescope init
codescope init --profile heavy
codescope init --force
```

## `codescope index`

Indexes the codebase (incremental by default):
- walks files, applies ignore rules, parses and chunks
- stores chunks/metadata in `.codescope/meta.sqlite`
- builds/updates `.codescope/tantivy/` (lexical search)
- builds/updates `.codescope/hnsw.index` (semantic search, if embeddings are enabled)

Options:
- `--all`: force full re-index (ignore change detection cache)
- `-j, --jobs <N>`: number of parallel worker jobs

Examples:

```bash
codescope index
codescope index --all
codescope index --jobs 8
```

If you see a semantic index error (ex: HNSW dimension mismatch), run:

```bash
codescope clean --yes
codescope index --all
```

### Vue (`.vue`) files

`.vue` single-file components are indexed by treating them as HTML for parsing/chunking, so they are not ignored by default. If embeddings are enabled during indexing, they are also embedded and available for `--type semantic` / `--type hybrid` search.

## `codescope search`

Searches the indexed codebase.

Arguments:
- `<QUERY>`: query string

Options:
- `-t, --type <lexical|semantic|hybrid>`: search mode (`hybrid` is default)
- `-n, --top <N>`: number of results (default: 10)
- `--pretty`: human-readable output (default is JSONL)

### Token optimization flags (for AI agents)

- `--compact`: output file paths and line ranges only (no code snippets)
- `--excerpt-lines <N>`: limit snippet output to N lines (default: full chunk)
- `--dedupe <true|false>`: deduplicate overlapping chunks (default: true)
- `--no-dedupe`: disable overlap deduplication (for debugging)

Examples:

```bash
codescope search "authentication middleware" --type lexical
codescope search "error handling" --type hybrid -n 20 --pretty
codescope search "vector database" --type semantic -n 5

# Token-efficient search for AI agents
codescope search "auth" --compact
codescope search "middleware" --excerpt-lines 5
codescope search "config" --no-dedupe
```

Exit codes:
- `0`: results printed
- `2`: no results
- `>0`: error (project not initialized, missing model files, etc.)

## `codescope trace`

Traces call graph relationships from the indexed metadata (best-effort cross-file resolution).

Subcommands:
- `codescope trace callers <SYMBOL> [--file <PATH>] [--pretty] [--compact]`
- `codescope trace callees <SYMBOL> [--file <PATH>] [--pretty] [--compact]`
- `codescope trace graph <SYMBOL> [--file <PATH>] [--depth <N>] [--format <jsonl|dot>] [--pretty] [--compact]`

Examples:

```bash
codescope trace callers "processOrder"
codescope trace callers "processOrder" --pretty
codescope trace callees "processOrder" --file src/order.ts
codescope trace graph "processOrder" --depth 3
codescope trace graph "processOrder" --depth 3 --pretty
codescope trace graph "processOrder" --format dot > graph.dot
```

Outputs:
- JSONL (default) for callers/callees/graph
- Graphviz DOT for `trace graph --format dot`
- `trace callees --pretty` prints a best-effort target label (`file:line`, or `project|builtin|stdlib|external|unresolved`)

Viewing DOT graphs (Graphviz):

```bash
dot -Tsvg graph.dot -o graph.svg
```

### Semantic/hybrid prerequisites (local model files)

`semantic` and `hybrid` require a local embedding model in ONNX format plus a `tokenizer.json`.

By default, codescope looks under:
- `%USERPROFILE%\.codescope\models\<model_id>\model.onnx`
- `%USERPROFILE%\.codescope\models\<model_id>\tokenizer.json`

The default `model_id` is `paraphrase-multilingual-MiniLM-L12-v2` (configured in `.codescope/config.toml` under `[embedding]`).

Recommended models:
- `paraphrase-multilingual-MiniLM-L12-v2`: better for non-English queries (French, etc.)
- `all-MiniLM-L6-v2`: fast baseline, often strong for English

Download (Windows PowerShell example):

```powershell
$model_id = "paraphrase-multilingual-MiniLM-L12-v2"
$dst = Join-Path $env:USERPROFILE (".codescope\\models\\" + $model_id)
New-Item -ItemType Directory -Force -Path $dst | Out-Null
Invoke-WebRequest -Uri "https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/onnx/model.onnx" -OutFile (Join-Path $dst "model.onnx")
Invoke-WebRequest -Uri "https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/tokenizer.json" -OutFile (Join-Path $dst "tokenizer.json")
```

You can override the model directory by setting `embedding.model_path` in `.codescope/config.toml` to the directory that contains `model.onnx` and `tokenizer.json` (absolute path recommended).

## `codescope status`

Shows project status and index stats (counts come from `.codescope/meta.sqlite` and the on-disk indexes).

Example:

```bash
codescope status
```

## `codescope clean`

Deletes index data and recreates an empty `.codescope/meta.sqlite`, while keeping `.codescope/config.toml`.

Options:
- `-y, --yes`: skip confirmation prompt

Examples:

```bash
codescope clean
codescope clean --yes
```

## `codescope agent-setup`

Outputs instructions for configuring AI agents (Claude, GPT, etc.) to use codescope effectively.

Example:

```bash
codescope agent-setup
```

This prints recommended CLAUDE.md / system prompt snippets with token-efficient search patterns.

## Inspecting `.codescope/meta.sqlite`

You can open it with a GUI client (DBeaver, DataGrip, etc.) or use `sqlite3`:

```bash
sqlite3 .codescope/meta.sqlite ".tables"
sqlite3 .codescope/meta.sqlite ".schema"
```

## Building / installing (local dev)

Debug build:

```bash
cargo build --workspace
```

Binary path:
- Windows: `target\debug\codescope.exe`

If you want `codescope` available globally on your machine during development, the simplest approach is to install it into Cargo’s bin directory:

```bash
cargo install --path crates/codescope-cli --locked --force
```

Then ensure `%USERPROFILE%\.cargo\bin` is in your `PATH`.
