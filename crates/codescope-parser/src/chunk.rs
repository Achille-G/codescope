//! Chunk representation for parsed code

use serde::{Deserialize, Serialize};

/// Kind of code chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    /// A function or standalone procedure
    Function,
    /// A method within a class/struct
    Method,
    /// A class definition
    Class,
    /// A struct definition
    Struct,
    /// An interface/trait definition
    Interface,
    /// A module or namespace
    Module,
    /// A fixed-size block (fallback chunking)
    Block,
}

impl ChunkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkKind::Function => "function",
            ChunkKind::Method => "method",
            ChunkKind::Class => "class",
            ChunkKind::Struct => "struct",
            ChunkKind::Interface => "interface",
            ChunkKind::Module => "module",
            ChunkKind::Block => "block",
        }
    }
}

impl std::fmt::Display for ChunkKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A chunk of code extracted from a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Symbol name (function name, class name, etc.)
    pub symbol: Option<String>,

    /// Kind of chunk
    pub kind: ChunkKind,

    /// Start line (1-indexed)
    pub start_line: u32,

    /// End line (1-indexed, inclusive)
    pub end_line: u32,

    /// The actual content of the chunk
    pub content: String,

    /// Optional parent symbol (e.g., class name for a method)
    pub parent: Option<String>,
}

impl Chunk {
    /// Create a new chunk
    pub fn new(
        symbol: Option<String>,
        kind: ChunkKind,
        start_line: u32,
        end_line: u32,
        content: String,
    ) -> Self {
        Self {
            symbol,
            kind,
            start_line,
            end_line,
            content,
            parent: None,
        }
    }

    /// Set the parent symbol
    pub fn with_parent(mut self, parent: String) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Get a display name for the chunk
    pub fn display_name(&self) -> String {
        match (&self.parent, &self.symbol) {
            (Some(parent), Some(symbol)) => format!("{}.{}", parent, symbol),
            (None, Some(symbol)) => symbol.clone(),
            _ => format!("{}:{}-{}", self.kind, self.start_line, self.end_line),
        }
    }

    /// Get the line count
    pub fn line_count(&self) -> u32 {
        self.end_line.saturating_sub(self.start_line) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_display_name() {
        let chunk = Chunk::new(
            Some("foo".to_string()),
            ChunkKind::Function,
            1,
            10,
            "fn foo() {}".to_string(),
        );
        assert_eq!(chunk.display_name(), "foo");

        let method = chunk.with_parent("MyClass".to_string());
        assert_eq!(method.display_name(), "MyClass.foo");
    }

    #[test]
    fn test_chunk_line_count() {
        let chunk = Chunk::new(None, ChunkKind::Block, 1, 10, "".to_string());
        assert_eq!(chunk.line_count(), 10);
    }
}
