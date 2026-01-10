//! Tree-sitter based parser

use crate::{Chunk, ChunkKind, Error, Language, Result};
use parking_lot::Mutex;
use std::collections::HashMap;

/// Thread-safe parser pool
pub struct Parser {
    parsers: Mutex<HashMap<Language, tree_sitter::Parser>>,
}

impl Parser {
    /// Create a new parser pool
    pub fn new() -> Self {
        Self {
            parsers: Mutex::new(HashMap::new()),
        }
    }

    /// Parse a file and extract chunks
    pub fn parse(&self, content: &str, language: Language) -> Result<Vec<Chunk>> {
        if !language.supports_ast_chunking() {
            return self.fallback_chunk(content);
        }

        let tree = self.parse_tree(content, language)?;
        self.extract_chunks(&tree, content, language)
    }

    /// Parse content into a tree-sitter tree
    fn parse_tree(&self, content: &str, language: Language) -> Result<tree_sitter::Tree> {
        let ts_lang = language.tree_sitter_language()?;

        let mut parsers = self.parsers.lock();
        let parser = parsers.entry(language).or_insert_with(|| {
            let mut p = tree_sitter::Parser::new();
            p.set_language(&ts_lang).expect("Failed to set language");
            p
        });

        parser
            .parse(content, None)
            .ok_or_else(|| Error::Parse("Failed to parse content".to_string()))
    }

    /// Extract chunks from a parsed tree
    fn extract_chunks(
        &self,
        tree: &tree_sitter::Tree,
        content: &str,
        language: Language,
    ) -> Result<Vec<Chunk>> {
        let mut chunks = Vec::new();
        let root = tree.root_node();

        self.visit_node(root, content, language, &mut chunks, None);

        // Sort by start line
        chunks.sort_by_key(|c| c.start_line);

        Ok(chunks)
    }

    /// Recursively visit nodes and extract chunks
    fn visit_node(
        &self,
        node: tree_sitter::Node,
        content: &str,
        language: Language,
        chunks: &mut Vec<Chunk>,
        parent_name: Option<&str>,
    ) {
        let kind = node.kind();

        // Check if this node is a chunkable entity
        if let Some((chunk_kind, name)) = self.node_to_chunk_info(node, content, language) {
            let start_line = node.start_position().row as u32 + 1;
            let end_line = node.end_position().row as u32 + 1;
            let chunk_content = node
                .utf8_text(content.as_bytes())
                .unwrap_or("")
                .to_string();

            // For classes, visit children with this class as parent
            let is_container = matches!(chunk_kind, ChunkKind::Class | ChunkKind::Struct | ChunkKind::Interface);
            let class_name = if is_container { name.clone() } else { None };

            let mut chunk = Chunk::new(name, chunk_kind, start_line, end_line, chunk_content);

            if let Some(parent) = parent_name {
                chunk = chunk.with_parent(parent.to_string());
            }

            chunks.push(chunk);

            if is_container {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_node(child, content, language, chunks, class_name.as_deref());
                }
                return;
            }
        }

        // Visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, content, language, chunks, parent_name);
        }
    }

    /// Determine if a node represents a chunkable entity
    fn node_to_chunk_info(
        &self,
        node: tree_sitter::Node,
        content: &str,
        language: Language,
    ) -> Option<(ChunkKind, Option<String>)> {
        let kind = node.kind();

        match language {
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                self.js_node_info(node, content, kind)
            }
            Language::Python => self.python_node_info(node, content, kind),
            Language::Rust => self.rust_node_info(node, content, kind),
            Language::Java => self.java_node_info(node, content, kind),
            Language::Go => self.go_node_info(node, content, kind),
            Language::C | Language::Cpp => self.c_node_info(node, content, kind),
            _ => None,
        }
    }

    fn js_node_info(
        &self,
        node: tree_sitter::Node,
        content: &str,
        kind: &str,
    ) -> Option<(ChunkKind, Option<String>)> {
        match kind {
            "function_declaration" | "function" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Function, name))
            }
            "arrow_function" | "function_expression" => {
                // Try to get name from variable declarator parent
                Some((ChunkKind::Function, None))
            }
            "method_definition" => {
                let name = self.find_child_text(node, "property_identifier", content);
                Some((ChunkKind::Method, name))
            }
            "class_declaration" | "class" => {
                let name = self.find_child_text(node, "identifier", content)
                    .or_else(|| self.find_child_text(node, "type_identifier", content));
                Some((ChunkKind::Class, name))
            }
            _ => None,
        }
    }

    fn python_node_info(
        &self,
        node: tree_sitter::Node,
        content: &str,
        kind: &str,
    ) -> Option<(ChunkKind, Option<String>)> {
        match kind {
            "function_definition" => {
                let name = self.find_child_text(node, "identifier", content);
                // Check if inside a class (has self parameter typically)
                let chunk_kind = if node
                    .parent()
                    .map(|p| p.kind() == "block")
                    .unwrap_or(false)
                    && node
                        .parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.kind() == "class_definition")
                        .unwrap_or(false)
                {
                    ChunkKind::Method
                } else {
                    ChunkKind::Function
                };
                Some((chunk_kind, name))
            }
            "class_definition" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Class, name))
            }
            _ => None,
        }
    }

    fn rust_node_info(
        &self,
        node: tree_sitter::Node,
        content: &str,
        kind: &str,
    ) -> Option<(ChunkKind, Option<String>)> {
        match kind {
            "function_item" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Function, name))
            }
            "impl_item" => {
                // Get the type name
                let name = self.find_child_text(node, "type_identifier", content);
                Some((ChunkKind::Class, name))
            }
            "struct_item" => {
                let name = self.find_child_text(node, "type_identifier", content);
                Some((ChunkKind::Struct, name))
            }
            "trait_item" => {
                let name = self.find_child_text(node, "type_identifier", content);
                Some((ChunkKind::Interface, name))
            }
            _ => None,
        }
    }

    fn java_node_info(
        &self,
        node: tree_sitter::Node,
        content: &str,
        kind: &str,
    ) -> Option<(ChunkKind, Option<String>)> {
        match kind {
            "method_declaration" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Method, name))
            }
            "constructor_declaration" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Method, name))
            }
            "class_declaration" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Class, name))
            }
            "interface_declaration" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Interface, name))
            }
            _ => None,
        }
    }

    fn go_node_info(
        &self,
        node: tree_sitter::Node,
        content: &str,
        kind: &str,
    ) -> Option<(ChunkKind, Option<String>)> {
        match kind {
            "function_declaration" => {
                let name = self.find_child_text(node, "identifier", content);
                Some((ChunkKind::Function, name))
            }
            "method_declaration" => {
                let name = self.find_child_text(node, "field_identifier", content);
                Some((ChunkKind::Method, name))
            }
            "type_declaration" => {
                // Could be struct or interface
                let name = self.find_child_text(node, "type_identifier", content);
                Some((ChunkKind::Struct, name))
            }
            _ => None,
        }
    }

    fn c_node_info(
        &self,
        node: tree_sitter::Node,
        content: &str,
        kind: &str,
    ) -> Option<(ChunkKind, Option<String>)> {
        match kind {
            "function_definition" => {
                // Name is in the declarator
                let name = node
                    .child_by_field_name("declarator")
                    .and_then(|d| self.find_child_text(d, "identifier", content));
                Some((ChunkKind::Function, name))
            }
            "struct_specifier" => {
                let name = self.find_child_text(node, "type_identifier", content);
                Some((ChunkKind::Struct, name))
            }
            _ => None,
        }
    }

    /// Find a child node with the given kind and return its text
    fn find_child_text(
        &self,
        node: tree_sitter::Node,
        child_kind: &str,
        content: &str,
    ) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == child_kind {
                return child.utf8_text(content.as_bytes()).ok().map(String::from);
            }
            // Also check grandchildren for nested structures
            let mut inner_cursor = child.walk();
            for grandchild in child.children(&mut inner_cursor) {
                if grandchild.kind() == child_kind {
                    return grandchild
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(String::from);
                }
            }
        }
        None
    }

    /// Fallback chunking for unsupported languages
    fn fallback_chunk(&self, content: &str) -> Result<Vec<Chunk>> {
        const CHUNK_SIZE: usize = 500;
        const OVERLAP: usize = 50;

        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();

        if lines.len() <= CHUNK_SIZE {
            // Single chunk for small files
            chunks.push(Chunk::new(
                None,
                ChunkKind::Block,
                1,
                lines.len() as u32,
                content.to_string(),
            ));
        } else {
            // Sliding window
            let mut start = 0;
            while start < lines.len() {
                let end = (start + CHUNK_SIZE).min(lines.len());
                let chunk_content = lines[start..end].join("\n");

                chunks.push(Chunk::new(
                    None,
                    ChunkKind::Block,
                    (start + 1) as u32,
                    end as u32,
                    chunk_content,
                ));

                if end >= lines.len() {
                    break;
                }
                start = end - OVERLAP;
            }
        }

        Ok(chunks)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_typescript() {
        let parser = Parser::new();
        let content = r#"
function hello(name: string): void {
    console.log(`Hello, ${name}!`);
}

class Greeter {
    greet(name: string): void {
        console.log(`Hi, ${name}!`);
    }
}
"#;

        let chunks = parser.parse(content, Language::TypeScript).unwrap();
        assert!(!chunks.is_empty());

        // Should have function and class
        let function = chunks.iter().find(|c| c.kind == ChunkKind::Function);
        assert!(function.is_some());
        assert_eq!(function.unwrap().symbol, Some("hello".to_string()));

        let class = chunks.iter().find(|c| c.kind == ChunkKind::Class);
        assert!(class.is_some());
        assert_eq!(class.unwrap().symbol, Some("Greeter".to_string()));
    }

    #[test]
    fn test_parse_python() {
        let parser = Parser::new();
        let content = r#"
def greet(name):
    print(f"Hello, {name}!")

class Greeter:
    def say_hi(self, name):
        print(f"Hi, {name}!")
"#;

        let chunks = parser.parse(content, Language::Python).unwrap();
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_fallback_chunking() {
        let parser = Parser::new();
        let content = "line1\nline2\nline3\nline4\nline5";

        let chunks = parser.fallback_chunk(content).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, ChunkKind::Block);
    }
}
