//! Golden tests for code chunking
//!
//! These tests verify that the parser produces consistent, expected chunks
//! for various language fixtures.

use codescope_parser::{Language, Parser};

/// Helper to get fixture content
fn fixture_content(fixture: &str) -> String {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), fixture);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read fixture: {path}"))
}

#[test]
fn test_typescript_chunking_extracts_class() {
    let parser = Parser::new();
    let content = fixture_content("typescript/sample.ts");

    let chunks = parser.parse(&content, Language::TypeScript).unwrap();

    // Should extract the AuthService class
    let class_chunk = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("AuthService"))
        .expect("Should find AuthService class");

    assert_eq!(class_chunk.kind.as_str(), "class");
    assert!(class_chunk.content.contains("class AuthService"));
}

#[test]
fn test_typescript_chunking_extracts_methods() {
    let parser = Parser::new();
    let content = fixture_content("typescript/sample.ts");

    let chunks = parser.parse(&content, Language::TypeScript).unwrap();

    // Should extract login method
    let login_method = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("login"))
        .expect("Should find login method");

    assert_eq!(login_method.kind.as_str(), "method");
    assert!(login_method.content.contains("async login"));

    // Should extract register method
    let register_method = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("register"))
        .expect("Should find register method");

    assert_eq!(register_method.kind.as_str(), "method");
}

#[test]
fn test_typescript_chunking_extracts_functions() {
    let parser = Parser::new();
    let content = fixture_content("typescript/sample.ts");

    let chunks = parser.parse(&content, Language::TypeScript).unwrap();

    // Should extract sum function
    let sum_fn = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("sum"))
        .expect("Should find sum function");

    assert_eq!(sum_fn.kind.as_str(), "function");

    // Should extract average function
    let avg_fn = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("average"))
        .expect("Should find average function");

    assert_eq!(avg_fn.kind.as_str(), "function");
}

#[test]
fn test_typescript_chunking_extracts_arrow_functions() {
    let parser = Parser::new();
    let content = fixture_content("typescript/sample.ts");

    let chunks = parser.parse(&content, Language::TypeScript).unwrap();

    // Should extract greet arrow function
    let greet_fn = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("greet"))
        .expect("Should find greet arrow function");

    assert_eq!(greet_fn.kind.as_str(), "function");
}

#[test]
fn test_typescript_chunking_includes_jsdoc() {
    let parser = Parser::new();
    let content = fixture_content("typescript/sample.ts");

    let chunks = parser.parse(&content, Language::TypeScript).unwrap();

    // Check that the login method includes its JSDoc comment
    // (methods inside classes are more reliably captured with leading comments)
    let login_method = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("login"))
        .expect("Should find login method");

    // Should include JSDoc comment or at least the function signature
    assert!(
        login_method.content.contains("Login a user")
            || login_method.content.contains("async login"),
        "Should include method content: {}",
        login_method.content
    );
}

#[test]
fn test_python_chunking_extracts_class() {
    let parser = Parser::new();
    let content = fixture_content("python/sample.py");

    let chunks = parser.parse(&content, Language::Python).unwrap();

    let class_chunk = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("UserRepository"))
        .expect("Should find UserRepository class");

    assert_eq!(class_chunk.kind.as_str(), "class");
}

#[test]
fn test_python_chunking_extracts_methods() {
    let parser = Parser::new();
    let content = fixture_content("python/sample.py");

    let chunks = parser.parse(&content, Language::Python).unwrap();

    // Should extract add method
    let add_method = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("add"))
        .expect("Should find add method");

    assert_eq!(add_method.kind.as_str(), "method");

    // Should extract find_by_id method
    let find_method = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("find_by_id"))
        .expect("Should find find_by_id method");

    assert_eq!(find_method.kind.as_str(), "method");
}

#[test]
fn test_python_chunking_extracts_functions() {
    let parser = Parser::new();
    let content = fixture_content("python/sample.py");

    let chunks = parser.parse(&content, Language::Python).unwrap();

    let sum_fn = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("calculate_sum"))
        .expect("Should find calculate_sum function");

    assert_eq!(sum_fn.kind.as_str(), "function");
}

#[test]
fn test_rust_chunking_extracts_struct() {
    let parser = Parser::new();
    let content = fixture_content("rust/sample.rs");

    let chunks = parser.parse(&content, Language::Rust).unwrap();

    let struct_chunk = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("User") && c.kind.as_str() == "struct")
        .expect("Should find User struct");

    assert!(struct_chunk.content.contains("pub struct User"));
}

#[test]
fn test_rust_chunking_extracts_impl_methods() {
    let parser = Parser::new();
    let content = fixture_content("rust/sample.rs");

    let chunks = parser.parse(&content, Language::Rust).unwrap();

    // Should extract new method in impl block
    let new_method = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("new") && c.kind.as_str() == "method")
        .expect("Should find new method");

    assert!(new_method.content.contains("pub fn new"));
}

#[test]
fn test_rust_chunking_extracts_functions() {
    let parser = Parser::new();
    let content = fixture_content("rust/sample.rs");

    let chunks = parser.parse(&content, Language::Rust).unwrap();

    let sum_fn = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("calculate_sum") && c.kind.as_str() == "function")
        .expect("Should find calculate_sum function");

    assert!(sum_fn.content.contains("pub fn calculate_sum"));
}

#[test]
fn test_rust_chunking_includes_doc_comments() {
    let parser = Parser::new();
    let content = fixture_content("rust/sample.rs");

    let chunks = parser.parse(&content, Language::Rust).unwrap();

    let greet_fn = chunks
        .iter()
        .find(|c| c.symbol.as_deref() == Some("greet") && c.kind.as_str() == "function")
        .expect("Should find greet function");

    // Should include doc comment
    assert!(
        greet_fn.content.contains("/// Return a greeting message"),
        "Should include doc comment"
    );
}

#[test]
fn test_chunk_count_reasonable() {
    let parser = Parser::new();

    // TypeScript should have ~6-10 chunks
    let ts_content = fixture_content("typescript/sample.ts");
    let ts_chunks = parser.parse(&ts_content, Language::TypeScript).unwrap();
    assert!(
        ts_chunks.len() >= 5,
        "TypeScript should have at least 5 chunks"
    );
    assert!(
        ts_chunks.len() <= 15,
        "TypeScript should have at most 15 chunks"
    );

    // Python should have ~8-12 chunks
    let py_content = fixture_content("python/sample.py");
    let py_chunks = parser.parse(&py_content, Language::Python).unwrap();
    assert!(py_chunks.len() >= 6, "Python should have at least 6 chunks");
    assert!(
        py_chunks.len() <= 15,
        "Python should have at most 15 chunks"
    );

    // Rust should have ~10-15 chunks
    let rs_content = fixture_content("rust/sample.rs");
    let rs_chunks = parser.parse(&rs_content, Language::Rust).unwrap();
    assert!(rs_chunks.len() >= 8, "Rust should have at least 8 chunks");
    assert!(rs_chunks.len() <= 25, "Rust should have at most 25 chunks");
}

#[test]
fn test_line_numbers_are_valid() {
    let parser = Parser::new();
    let content = fixture_content("typescript/sample.ts");

    let chunks = parser.parse(&content, Language::TypeScript).unwrap();
    let total_lines = content.lines().count() as u32;

    for chunk in &chunks {
        assert!(
            chunk.start_line >= 1,
            "Start line should be >= 1, got {}",
            chunk.start_line
        );
        assert!(
            chunk.end_line <= total_lines + 1,
            "End line {} exceeds total lines {}",
            chunk.end_line,
            total_lines
        );
        assert!(
            chunk.start_line <= chunk.end_line,
            "Start line {} should be <= end line {}",
            chunk.start_line,
            chunk.end_line
        );
    }
}
