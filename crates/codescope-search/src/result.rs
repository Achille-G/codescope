//! Search result types

use serde::{Deserialize, Serialize};

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
        serde_json::to_string(self).unwrap_or_default()
    }
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
}
