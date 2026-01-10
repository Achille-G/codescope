# CLI Reference

`codescope` is a per-repo CLI. It stores all per-project state in a `.codescope/` directory at the project root.

## Common options

- `-v, --verbose`: more logs (debug for codescope crates)
- `-q, --quiet`: errors only

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

Examples:

```bash
codescope search "authentication middleware" --type lexical
codescope search "error handling" --type hybrid -n 20 --pretty
codescope search "vector database" --type semantic -n 5
```

Exit codes:
- `0`: results printed
- `2`: no results
- `>0`: error (project not initialized, missing model files, etc.)

### Semantic/hybrid prerequisites (local model files)

`semantic` and `hybrid` require a local embedding model in ONNX format plus a `tokenizer.json`.

By default, codescope looks under:
- `%USERPROFILE%\.codescope\models\<model_id>\model.onnx`
- `%USERPROFILE%\.codescope\models\<model_id>\tokenizer.json`

The default `model_id` is `all-MiniLM-L6-v2` (configured in `.codescope/config.toml` under `[embedding]`).

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
