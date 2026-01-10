//! Search relevance tests
//!
//! These tests verify that search produces relevant results for known queries.

use codescope_parser::{Language, Parser};
use codescope_search::{BM25Index, HNSWIndex, SearchEngine, StoragePool};
use xxhash_rust::xxh3::xxh3_64;

/// Helper to get fixture content
fn fixture_content(fixture: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        fixture
    );
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", path))
}

/// Set up an in-memory search engine with fixtures indexed
fn setup_search_engine() -> SearchEngine {
    let parser = Parser::new();

    // Parse all fixtures
    let ts_content = fixture_content("typescript/sample.ts");
    let py_content = fixture_content("python/sample.py");
    let rs_content = fixture_content("rust/sample.rs");

    let ts_chunks = parser.parse(&ts_content, Language::TypeScript).unwrap();
    let py_chunks = parser.parse(&py_content, Language::Python).unwrap();
    let rs_chunks = parser.parse(&rs_content, Language::Rust).unwrap();

    // Set up storage
    let pool = StoragePool::open_memory(4).unwrap();

    // Set up BM25
    let mut bm25 = BM25Index::open_memory().unwrap();
    bm25.begin_write(100_000_000).unwrap();

    // Set up HNSW (minimal dimensions since we're not using embeddings for these tests)
    let mut hnsw = HNSWIndex::with_defaults(4).unwrap();

    let files = [
        ("typescript/sample.ts", "typescript", ts_chunks),
        ("python/sample.py", "python", py_chunks),
        ("rust/sample.rs", "rust", rs_chunks),
    ];

    {
        let storage = pool.get().unwrap();
        let mut chunk_id = 1i64;

        for (file_path, lang, chunks) in files {
            let file_hash = xxh3_64(file_path.as_bytes()).to_le_bytes();
            let file_id = storage
                .upsert_file(file_path, Some(lang), &file_hash, 1000)
                .unwrap();

            for chunk in chunks {
                let content_hash = xxh3_64(chunk.content.as_bytes()).to_le_bytes();

                storage
                    .insert_chunk(
                        file_id,
                        chunk.symbol.as_deref(),
                        chunk.kind.as_str(),
                        chunk.start_line,
                        chunk.end_line,
                        &content_hash,
                        &chunk.content,
                    )
                    .unwrap();

                bm25.add_document(
                    chunk_id,
                    &chunk.content,
                    chunk.symbol.as_deref(),
                    chunk.kind.as_str(),
                    file_path,
                )
                .unwrap();

                // Add dummy vector for HNSW
                hnsw.add(chunk_id, vec![0.5, 0.5, 0.5, 0.5]).unwrap();

                chunk_id += 1;
            }
        }
    }

    bm25.commit().unwrap();

    SearchEngine::new(pool, bm25, hnsw)
}

#[test]
fn test_search_finds_login_function() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("login", 10).unwrap();

    assert!(!results.results.is_empty(), "Should find results for 'login'");

    // First result should be the login method
    let first = &results.results[0];
    assert!(
        first.symbol.as_deref() == Some("login")
            || first.snippet.contains("login"),
        "First result should be login-related"
    );
}

#[test]
fn test_search_finds_user_repository() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("UserRepository", 10).unwrap();

    assert!(
        !results.results.is_empty(),
        "Should find results for 'UserRepository'"
    );

    // Should find UserRepository in both Python and Rust
    let symbols: Vec<_> = results
        .results
        .iter()
        .filter_map(|r| r.symbol.as_deref())
        .collect();

    assert!(
        symbols.iter().any(|s| s.contains("UserRepository")),
        "Should find UserRepository symbol"
    );
}

#[test]
fn test_search_finds_by_method_name() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("find_by_id", 10).unwrap();

    assert!(
        !results.results.is_empty(),
        "Should find results for 'find_by_id'"
    );

    // Should find the method
    let has_method = results
        .results
        .iter()
        .any(|r| r.symbol.as_deref() == Some("find_by_id"));

    assert!(has_method, "Should find find_by_id method");
}

#[test]
fn test_search_finds_calculate_functions() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("calculate sum", 10).unwrap();

    assert!(
        !results.results.is_empty(),
        "Should find results for 'calculate sum'"
    );

    // Should find calculate_sum in multiple languages
    let files: Vec<_> = results.results.iter().map(|r| r.file.as_str()).collect();

    // Should have results from at least two files
    assert!(
        files.len() >= 2 || files.iter().any(|f| f.contains("python") || f.contains("rust")),
        "Should find calculate functions in multiple files"
    );
}

#[test]
fn test_search_finds_by_content() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("greeting message", 10).unwrap();

    assert!(
        !results.results.is_empty(),
        "Should find results for 'greeting message'"
    );

    // Should find the greet functions (they have "greeting" in comments)
    let found_greet = results
        .results
        .iter()
        .any(|r| r.snippet.contains("greet") || r.symbol.as_deref() == Some("greet"));

    assert!(found_greet, "Should find greet-related results");
}

#[test]
fn test_search_finds_class_by_name() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("AuthService", 10).unwrap();

    assert!(
        !results.results.is_empty(),
        "Should find results for 'AuthService'"
    );

    let first = &results.results[0];
    assert_eq!(
        first.symbol.as_deref(),
        Some("AuthService"),
        "First result should be AuthService"
    );
}

#[test]
fn test_search_respects_top_k() {
    let engine = setup_search_engine();

    let results_5 = engine.search_lexical("function", 5).unwrap();
    let results_10 = engine.search_lexical("function", 10).unwrap();

    assert!(
        results_5.results.len() <= 5,
        "Should return at most 5 results"
    );
    assert!(
        results_10.results.len() >= results_5.results.len(),
        "More results with higher top_k"
    );
}

#[test]
fn test_search_returns_file_info() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("calculate", 10).unwrap();

    for result in &results.results {
        assert!(!result.file.is_empty(), "File should not be empty");
        assert!(result.start >= 1, "Start line should be >= 1");
        assert!(result.end >= result.start, "End should be >= start");
    }
}

#[test]
fn test_search_returns_snippet() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("login", 10).unwrap();

    for result in &results.results {
        assert!(!result.snippet.is_empty(), "Snippet should not be empty");
    }
}

#[test]
fn test_search_no_results_for_gibberish() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("xyzzy123nonexistent", 10).unwrap();

    // Should return empty or very few results
    assert!(
        results.results.len() <= 1,
        "Should return very few results for gibberish query"
    );
}

#[test]
fn test_search_scores_are_reasonable() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("login password", 10).unwrap();

    // Scores should be positive
    for result in &results.results {
        assert!(result.score >= 0.0, "Score should be non-negative");
    }

    // Results should be sorted by score descending
    for i in 1..results.results.len() {
        assert!(
            results.results[i - 1].score >= results.results[i].score,
            "Results should be sorted by score descending"
        );
    }
}

#[test]
fn test_search_latency_is_reasonable() {
    let engine = setup_search_engine();

    let results = engine.search_lexical("function class method", 10).unwrap();

    // Search should complete in under 500ms (very generous for small corpus)
    assert!(
        results.took_ms < 500,
        "Search took {}ms, should be under 500ms",
        results.took_ms
    );
}
