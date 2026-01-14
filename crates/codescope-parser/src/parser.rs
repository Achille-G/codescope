//! Tree-sitter based parser

use crate::{Chunk, ChunkKind, Error, Import, Language, Result};
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
        let chunks = self.extract_chunks(&tree, content, language)?;
        if chunks.is_empty() {
            return self.fallback_chunk(content);
        }
        Ok(chunks)
    }

    /// Parse a file and extract chunks + imports.
    pub fn parse_with_imports(
        &self,
        content: &str,
        language: Language,
    ) -> Result<(Vec<Chunk>, Vec<Import>)> {
        if !language.supports_ast_chunking() {
            let chunks = self.fallback_chunk(content)?;
            return Ok((chunks, Vec::new()));
        }

        let tree = self.parse_tree(content, language)?;
        let imports = crate::import::extract_imports(tree.root_node(), content, language);
        let mut chunks = self.extract_chunks(&tree, content, language)?;

        if chunks.is_empty() {
            chunks = self.fallback_chunk(content)?;
            if let Some(chunk) = chunks.first_mut() {
                // Best-effort: top-level calls in files without chunkable symbols.
                chunk.call_sites =
                    self.extract_call_sites_scoped(tree.root_node(), content, language);
            }
        }

        Ok((chunks, imports))
    }

    /// Parse content into a tree-sitter tree
    fn parse_tree(&self, content: &str, language: Language) -> Result<tree_sitter::Tree> {
        let ts_lang = language.tree_sitter_language()?;

        let mut parsers = self.parsers.lock();
        let parser = parsers.entry(language).or_default();
        parser
            .set_language(&ts_lang)
            .map_err(|err| Error::TreeSitter(err.to_string()))?;

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
        // Check if this node is a chunkable entity
        if let Some((chunk_kind, name)) = self.node_to_chunk_info(node, content, language) {
            let (start_line, chunk_content) = self.chunk_text_with_comments(node, content);
            let end_line = node.end_position().row as u32 + 1;

            // For classes, visit children with this class as parent
            let is_container = matches!(
                chunk_kind,
                ChunkKind::Class | ChunkKind::Struct | ChunkKind::Interface
            );
            let class_name = if is_container { name.clone() } else { None };

            let mut chunk = Chunk::new(name, chunk_kind, start_line, end_line, chunk_content);
            if matches!(chunk_kind, ChunkKind::Function | ChunkKind::Method) {
                chunk.call_sites = self.extract_call_sites_scoped(node, content, language);
            }

            if let Some(parent) = parent_name {
                chunk = chunk.with_parent(parent.to_string());
            } else if let Some(parent) = self.infer_parent_symbol(node, language, content) {
                chunk = chunk.with_parent(parent);
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

    fn extract_call_sites_scoped(
        &self,
        root: tree_sitter::Node,
        content: &str,
        language: Language,
    ) -> Vec<crate::CallSite> {
        let mut sites = Vec::new();
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            if node != root && self.node_to_chunk_info(node, content, language).is_some() {
                continue;
            }

            if let Some(site) = crate::call_site::extract_call_site(node, content, language) {
                sites.push(site);
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }

        sites
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
                let name = self.js_enclosing_name(node, content);
                Some((ChunkKind::Function, name))
            }
            "method_definition" => {
                let name = self.find_child_text(node, "property_identifier", content);
                Some((ChunkKind::Method, name))
            }
            "class_declaration" | "class" => {
                let name = self
                    .find_child_text(node, "identifier", content)
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
                let chunk_kind = if node.parent().map(|p| p.kind() == "block").unwrap_or(false)
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
                let is_method = self.has_ancestor(node, &["impl_item", "trait_item"]);
                let chunk_kind = if is_method {
                    ChunkKind::Method
                } else {
                    ChunkKind::Function
                };
                Some((chunk_kind, name))
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

    fn find_descendant_text(
        &self,
        node: tree_sitter::Node,
        kinds: &[&str],
        content: &str,
    ) -> Option<String> {
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            if kinds.iter().any(|kind| current.kind() == *kind) {
                if let Ok(text) = current.utf8_text(content.as_bytes()) {
                    return Some(text.to_string());
                }
            }
            let mut cursor = current.walk();
            for child in current.children(&mut cursor) {
                stack.push(child);
            }
        }
        None
    }

    fn has_ancestor(&self, node: tree_sitter::Node, kinds: &[&str]) -> bool {
        let mut current = node.parent();
        while let Some(parent) = current {
            if kinds.iter().any(|kind| parent.kind() == *kind) {
                return true;
            }
            current = parent.parent();
        }
        false
    }

    fn js_enclosing_name(&self, node: tree_sitter::Node, content: &str) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            match parent.kind() {
                "variable_declarator" => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        if let Ok(text) = name_node.utf8_text(content.as_bytes()) {
                            return Some(text.to_string());
                        }
                    }
                    return self
                        .find_child_text(parent, "identifier", content)
                        .or_else(|| self.find_child_text(parent, "property_identifier", content));
                }
                "assignment_expression" => {
                    if let Some(left) = parent.child_by_field_name("left") {
                        if let Some(name) = self.js_assignment_target_name(left, content) {
                            return Some(name);
                        }
                    }
                }
                "pair"
                | "property_assignment"
                | "property_definition"
                | "public_field_definition"
                | "field_definition" => {
                    if let Some(name) = self
                        .find_child_text(parent, "property_identifier", content)
                        .or_else(|| self.find_child_text(parent, "identifier", content))
                        .or_else(|| self.find_child_text(parent, "string", content))
                    {
                        return Some(name);
                    }
                }
                _ => {}
            }
            current = parent;
        }
        None
    }

    fn js_assignment_target_name(&self, node: tree_sitter::Node, content: &str) -> Option<String> {
        match node.kind() {
            "identifier" => node.utf8_text(content.as_bytes()).ok().map(String::from),
            "member_expression" => node
                .child_by_field_name("property")
                .and_then(|property| property.utf8_text(content.as_bytes()).ok())
                .map(String::from),
            _ => None,
        }
    }

    fn infer_parent_symbol(
        &self,
        node: tree_sitter::Node,
        language: Language,
        content: &str,
    ) -> Option<String> {
        match language {
            Language::Go => {
                if node.kind() == "method_declaration" {
                    self.go_receiver_type(node, content)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn go_receiver_type(&self, node: tree_sitter::Node, content: &str) -> Option<String> {
        let receiver = node.child_by_field_name("receiver")?;
        self.find_descendant_text(receiver, &["type_identifier", "qualified_type"], content)
    }

    fn chunk_text_with_comments(&self, node: tree_sitter::Node, content: &str) -> (u32, String) {
        let start_byte = self.leading_comment_start(node, content);
        let end_byte = node.end_byte();
        let start_line = if start_byte == node.start_byte() {
            node.start_position().row as u32 + 1
        } else {
            Self::line_number_at_byte(content, start_byte)
        };
        let chunk = content.get(start_byte..end_byte).unwrap_or("");
        (start_line, chunk.to_string())
    }

    fn leading_comment_start(&self, node: tree_sitter::Node, content: &str) -> usize {
        let mut start = node.start_byte();
        let mut prev = node.prev_sibling();
        while let Some(comment) = prev {
            if !Self::is_comment_node(comment.kind()) {
                break;
            }
            if !Self::is_whitespace_only(content, comment.end_byte(), start) {
                break;
            }
            start = comment.start_byte();
            prev = comment.prev_sibling();
        }
        start
    }

    fn is_comment_node(kind: &str) -> bool {
        kind.contains("comment")
    }

    fn is_whitespace_only(content: &str, start: usize, end: usize) -> bool {
        if start >= end || start >= content.len() {
            return true;
        }
        let end = end.min(content.len());
        content.as_bytes()[start..end]
            .iter()
            .all(|byte| byte.is_ascii_whitespace())
    }

    fn line_number_at_byte(content: &str, offset: usize) -> u32 {
        let end = offset.min(content.len());
        let count = content.as_bytes()[..end]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        count as u32 + 1
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

    fn assert_has_kind(parser: &Parser, language: Language, content: &str, kind: ChunkKind) {
        let chunks = parser.parse(content, language).unwrap();
        assert!(
            chunks.iter().any(|chunk| chunk.kind == kind),
            "expected {kind:?} chunk for {language:?}",
        );
    }

    fn assert_fallback(parser: &Parser, language: Language, content: &str) {
        let chunks = parser.parse(content, language).unwrap();
        assert!(!chunks.is_empty(), "expected chunks for {language:?}");
        assert!(
            chunks.iter().all(|chunk| chunk.kind == ChunkKind::Block),
            "expected fallback chunks for {language:?}",
        );
    }

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
    fn test_parse_all_languages() {
        let parser = Parser::new();

        assert_has_kind(
            &parser,
            Language::TypeScript,
            "function hello() {}",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::JavaScript,
            "function hello() {}",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::Tsx,
            "const App = () => <div>Hello</div>;",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::Jsx,
            "function App() { return <div />; }",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::Python,
            "def hello():\n    return 1",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::Rust,
            "fn hello() {}",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::Java,
            "class Greeter { void hello() {} }",
            ChunkKind::Class,
        );
        assert_has_kind(
            &parser,
            Language::C,
            "int hello() { return 0; }",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::Cpp,
            "int hello() { return 0; }",
            ChunkKind::Function,
        );
        assert_has_kind(
            &parser,
            Language::Go,
            "func hello() { }",
            ChunkKind::Function,
        );

        assert_fallback(&parser, Language::Html, "<div><p>Hi</p></div>");
        assert_fallback(&parser, Language::Css, ".a { color: red; }");
        assert_fallback(
            &parser,
            Language::Scss,
            "$color: red; .a { color: $color; }",
        );
        assert_fallback(&parser, Language::Json, "{ \"a\": 1 }");
        assert_fallback(&parser, Language::Yaml, "a: 1");
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
    fn test_js_arrow_function_name() {
        let parser = Parser::new();
        let content = "const greet = () => { return 1; };";

        let chunks = parser.parse(content, Language::JavaScript).unwrap();
        let function = chunks
            .iter()
            .find(|c| c.kind == ChunkKind::Function)
            .unwrap();
        assert_eq!(function.symbol.as_deref(), Some("greet"));
    }

    #[test]
    fn test_rust_method_parent() {
        let parser = Parser::new();
        let content = "struct Greeter {}\nimpl Greeter { fn hello(&self) {} }";

        let chunks = parser.parse(content, Language::Rust).unwrap();
        let method = chunks.iter().find(|c| c.kind == ChunkKind::Method).unwrap();
        assert_eq!(method.parent.as_deref(), Some("Greeter"));
    }

    #[test]
    fn test_go_method_parent() {
        let parser = Parser::new();
        let content = "type Greeter struct {}\nfunc (g *Greeter) Hello() {}";

        let chunks = parser.parse(content, Language::Go).unwrap();
        let method = chunks.iter().find(|c| c.kind == ChunkKind::Method).unwrap();
        assert_eq!(method.parent.as_deref(), Some("Greeter"));
    }

    #[test]
    fn test_leading_comment_included() {
        let parser = Parser::new();
        let content = "// doc\nfn hello() {}\n";

        let chunks = parser.parse(content, Language::Rust).unwrap();
        let function = chunks
            .iter()
            .find(|c| c.kind == ChunkKind::Function)
            .unwrap();
        assert!(function.content.starts_with("// doc"));
        assert_eq!(function.start_line, 1);
    }

    #[test]
    fn test_fallback_chunking() {
        let parser = Parser::new();
        let content = "line1\nline2\nline3\nline4\nline5";

        let chunks = parser.fallback_chunk(content).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, ChunkKind::Block);
    }

    #[test]
    fn test_fallback_chunking_overlap() {
        let parser = Parser::new();
        let lines: Vec<String> = (1..=600).map(|i| format!("line{i}")).collect();
        let content = lines.join("\n");

        let chunks = parser.fallback_chunk(&content).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 500);
        assert_eq!(chunks[1].start_line, 451);
        assert_eq!(chunks[1].end_line, 600);
    }
}
