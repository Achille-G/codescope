//! Search result types

use serde::{Deserialize, Serialize};

use crate::dedupe::ChunkDeduplicator;

/// A single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// File path relative to project root
    pub file: String,

    /// Symbol name (function, class, etc.)
    pub symbol: Option<String>,

    /// Kind of chunk (function, class, method, block)
    pub kind: String,

    /// Start line (1-indexed)
    pub start: u32,

    /// End line (1-indexed)
    pub end: u32,

    /// Relevance score (higher is better)
    pub score: f32,

    /// Code snippet (truncated for display)
    pub snippet: String,

    /// Chunk ID in the database
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<i64>,
}

impl SearchResult {
    /// Create a new search result
    pub fn new(
        file: String,
        symbol: Option<String>,
        kind: String,
        start: u32,
        end: u32,
        score: f32,
        snippet: String,
    ) -> Self {
        Self {
            file,
            symbol,
            kind,
            start,
            end,
            score,
            snippet,
            chunk_id: None,
        }
    }

    /// Set the chunk ID
    pub fn with_chunk_id(mut self, id: i64) -> Self {
        self.chunk_id = Some(id);
        self
    }

    /// Get a truncated snippet for display
    pub fn truncated_snippet(&self, max_lines: usize) -> String {
        let lines: Vec<&str> = self.snippet.lines().collect();
        if lines.len() <= max_lines {
            self.snippet.clone()
        } else {
            let mut result: String = lines[..max_lines].join("\n");
            result.push_str("\n...");
            result
        }
    }

    /// Format as JSONL line
    pub fn to_jsonl(&self) -> String {
        serialize_jsonl(self)
    }

    /// Format as compact JSONL (no snippet) for token optimization
    pub fn to_compact_jsonl(&self) -> String {
        // Create a minimal result without snippet for token savings
        let compact = CompactSearchResult {
            file: &self.file,
            symbol: self.symbol.as_deref(),
            kind: &self.kind,
            start: self.start,
            end: self.end,
            score: self.score,
        };
        serialize_jsonl(&compact)
    }

    /// Format as JSONL with truncated snippet
    pub fn to_jsonl_with_limit(&self, max_lines: usize) -> String {
        let mut result = self.clone();
        if self.snippet.lines().count() > max_lines {
            result.snippet = self.truncated_snippet(max_lines);
        }
        serialize_jsonl(&result)
    }
}

/// Serialize a value to a JSONL line, logging instead of silently
/// swallowing serialization failures.
fn serialize_jsonl<T: Serialize>(value: &T) -> String {
    match serde_json::to_string(value) {
        Ok(line) => line,
        Err(err) => {
            tracing::error!("Failed to serialize search result to JSON: {err}");
            String::new()
        }
    }
}

/// Compact result without snippet (for token optimization)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompactSearchResult<'a> {
    pub file: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<&'a str>,
    pub kind: &'a str,
    pub start: u32,
    pub end: u32,
    pub score: f32,
}

/// Search results with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// The search query
    pub query: String,

    /// Search type (lexical, semantic, hybrid)
    pub search_type: SearchType,

    /// Number of results
    pub count: usize,

    /// Time taken in milliseconds
    pub took_ms: u64,

    /// The results
    pub results: Vec<SearchResult>,
}

impl SearchResults {
    /// Remove overlapping chunks (>50% overlap), keeping earlier chunks.
    pub fn deduplicate(&mut self, overlap_threshold: f64) {
        let deduper = ChunkDeduplicator::new(overlap_threshold);
        let results = std::mem::take(&mut self.results);
        self.results = deduper.deduplicate(results);
        self.count = self.results.len();
    }
}

/// Type of search performed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchType {
    Lexical,
    Semantic,
    Hybrid,
}

impl std::fmt::Display for SearchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchType::Lexical => write!(f, "lexical"),
            SearchType::Semantic => write!(f, "semantic"),
            SearchType::Hybrid => write!(f, "hybrid"),
        }
    }
}

impl std::str::FromStr for SearchType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lexical" | "bm25" => Ok(SearchType::Lexical),
            "semantic" | "vector" | "ann" => Ok(SearchType::Semantic),
            "hybrid" => Ok(SearchType::Hybrid),
            _ => Err(format!("Unknown search type: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_jsonl() {
        let result = SearchResult::new(
            "src/main.rs".to_string(),
            Some("main".to_string()),
            "function".to_string(),
            1,
            10,
            0.95,
            "fn main() { }".to_string(),
        );

        let jsonl = result.to_jsonl();
        assert!(jsonl.contains("src/main.rs"));
        assert!(jsonl.contains("main"));
    }

    #[test]
    fn test_truncated_snippet() {
        let result = SearchResult::new(
            "test.rs".to_string(),
            None,
            "block".to_string(),
            1,
            100,
            0.5,
            "line1\nline2\nline3\nline4\nline5".to_string(),
        );

        let truncated = result.truncated_snippet(3);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_deduplication_removes_overlapping() {
        let mut results = SearchResults {
            query: "test".to_string(),
            search_type: SearchType::Hybrid,
            count: 3,
            took_ms: 10,
            results: vec![
                SearchResult::new(
                    "auth.js".to_string(),
                    None,
                    "function".to_string(),
                    1,
                    50,
                    0.9,
                    "function auth() { }".to_string(),
                ),
                // 20-70 overlaps 1-50 by 31 lines (62% of first chunk) - should be removed
                SearchResult::new(
                    "auth.js".to_string(),
                    None,
                    "function".to_string(),
                    20,
                    70,
                    0.8,
                    "function auth() { more code }".to_string(),
                ),
                SearchResult::new(
                    "other.js".to_string(),
                    None,
                    "function".to_string(),
                    1,
                    50,
                    0.7,
                    "function other() { }".to_string(),
                ),
            ],
        };

        results.deduplicate(0.5);

        // Should keep auth.js:1-50 and other.js (different file)
        assert_eq!(results.count, 2);
        assert_eq!(results.results.len(), 2);
        assert!(results
            .results
            .iter()
            .any(|r| r.file == "auth.js" && r.start == 1));
        assert!(results.results.iter().any(|r| r.file == "other.js"));
    }

    #[test]
    fn test_compact_jsonl_no_snippet() {
        let result = SearchResult::new(
            "src/main.rs".to_string(),
            Some("main".to_string()),
            "function".to_string(),
            1,
            10,
            0.95,
            "fn main() { println!(\"hello\"); }".to_string(),
        );

        let compact = result.to_compact_jsonl();
        let parsed: serde_json::Value = serde_json::from_str(&compact).unwrap();

        assert_eq!(parsed["file"], "src/main.rs");
        assert_eq!(parsed["symbol"], "main");
        assert!(parsed["snippet"].is_null());
    }

    #[test]
    fn test_jsonl_with_limit() {
        let result = SearchResult::new(
            "test.rs".to_string(),
            None,
            "function".to_string(),
            1,
            100,
            0.5,
            "line1\nline2\nline3\nline4\nline5\nline6".to_string(),
        );

        let jsonl = result.to_jsonl_with_limit(3);
        let parsed: serde_json::Value = serde_json::from_str(&jsonl).unwrap();

        // Should be truncated
        assert!(parsed["snippet"].as_str().unwrap().contains("..."));
    }
}
