//! Import extraction from Tree-sitter AST nodes.

use crate::Language;
use serde::{Deserialize, Serialize};

/// An imported symbol or module binding (best-effort, per imported binding).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// Module source, package, or include path.
    pub source: String,
    /// Imported symbol name (if applicable).
    pub symbol: Option<String>,
    /// Local alias or binding name (if applicable).
    pub alias: Option<String>,
    /// Whether this import is the language's "default import" form.
    pub is_default: bool,
}

impl Import {
    pub fn new(
        source: String,
        symbol: Option<String>,
        alias: Option<String>,
        is_default: bool,
    ) -> Self {
        Self {
            source,
            symbol,
            alias,
            is_default,
        }
    }
}

pub fn extract_imports(node: tree_sitter::Node, content: &str, language: Language) -> Vec<Import> {
    match language {
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
            collect(node, |n| js_import(n, content))
        }
        Language::Python => collect(node, |n| python_import(n, content)),
        Language::Rust => collect(node, |n| rust_import(n, content)),
        Language::Java => collect(node, |n| java_import(n, content)),
        Language::Go => collect(node, |n| go_import(n, content)),
        Language::C | Language::Cpp => collect(node, |n| c_import(n, content)),
        _ => Vec::new(),
    }
}

fn collect(
    node: tree_sitter::Node,
    mut parse: impl FnMut(tree_sitter::Node) -> Vec<Import>,
) -> Vec<Import> {
    let mut stack = vec![node];
    let mut out = Vec::new();

    while let Some(current) = stack.pop() {
        out.extend(parse(current));

        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }

    out
}

fn node_text(node: tree_sitter::Node, content: &str) -> Option<String> {
    node.utf8_text(content.as_bytes())
        .ok()
        .map(|s| s.to_string())
}

fn unquote(text: &str) -> String {
    let trimmed = text.trim();
    let trimmed = trimmed
        .trim_start_matches(['"', '\'', '`', '<'])
        .trim_end_matches(['"', '\'', '`', '>']);
    trimmed.to_string()
}

// --- JavaScript / TypeScript ---

fn js_import(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    match node.kind() {
        "import_statement" => js_import_statement(node, content),
        "call_expression" => js_require_call(node, content),
        _ => Vec::new(),
    }
}

fn js_import_statement(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    let mut cursor = node.walk();
    let source = node
        .children(&mut cursor)
        .find(|c| c.kind() == "string")
        .and_then(|s| node_text(s, content))
        .map(|s| unquote(&s));
    let Some(source) = source else {
        return Vec::new();
    };

    // Side-effect only import: `import "x";`
    let mut cursor = node.walk();
    let Some(import_clause) = node
        .children(&mut cursor)
        .find(|c| c.kind() == "import_clause")
    else {
        return Vec::new();
    };

    let mut imports = Vec::new();

    // Default import (first identifier in import_clause).
    let mut clause_cursor = import_clause.walk();
    if let Some(default_ident) = import_clause
        .children(&mut clause_cursor)
        .find(|c| c.kind() == "identifier")
        .and_then(|n| node_text(n, content))
    {
        imports.push(Import::new(source.clone(), None, Some(default_ident), true));
    }

    // Namespace import: `* as ns`.
    let mut clause_cursor = import_clause.walk();
    if let Some(namespace_import) = import_clause
        .children(&mut clause_cursor)
        .find(|c| c.kind() == "namespace_import")
    {
        let mut ns_cursor = namespace_import.walk();
        let alias = namespace_import
            .children(&mut ns_cursor)
            .find(|c| c.kind() == "identifier")
            .and_then(|n| node_text(n, content));
        if let Some(alias) = alias {
            imports.push(Import::new(
                source.clone(),
                Some("*".to_string()),
                Some(alias),
                false,
            ));
        }
    }

    // Named imports: `import { a as b, c } from "x"`.
    let mut clause_cursor = import_clause.walk();
    for child in import_clause.children(&mut clause_cursor) {
        if child.kind() != "named_imports" {
            continue;
        }

        let mut named_cursor = child.walk();
        for spec in child.children(&mut named_cursor) {
            if spec.kind() != "import_specifier" {
                continue;
            }

            let name = if let Some(name_node) = spec.child_by_field_name("name") {
                node_text(name_node, content)
            } else {
                let mut sc = spec.walk();
                let first_ident = spec.children(&mut sc).find(|n| n.kind() == "identifier");
                first_ident.and_then(|n| node_text(n, content))
            };

            let alias = spec
                .child_by_field_name("alias")
                .and_then(|n| node_text(n, content))
                .or_else(|| {
                    let mut sc = spec.walk();
                    let mut idents: Vec<String> = spec
                        .children(&mut sc)
                        .filter(|n| n.kind() == "identifier")
                        .filter_map(|n| node_text(n, content))
                        .collect();
                    if idents.len() >= 2 {
                        Some(idents.remove(1))
                    } else {
                        None
                    }
                });

            if let Some(name) = name {
                imports.push(Import::new(source.clone(), Some(name), alias, false));
            }
        }
    }

    imports
}

fn js_require_call(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    let Some(function) = node.child_by_field_name("function") else {
        return Vec::new();
    };
    let Some(function_name) = node_text(function, content) else {
        return Vec::new();
    };
    if function_name != "require" {
        return Vec::new();
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return Vec::new();
    };

    let mut args_cursor = args.walk();
    let Some(source) = args
        .children(&mut args_cursor)
        .find(|c| c.kind() == "string")
        .and_then(|n| node_text(n, content))
        .map(|s| unquote(&s))
    else {
        return Vec::new();
    };

    // Best-effort binding extraction:
    // - `const ns = require("./m")` => namespace binding
    // - `const { a, b: c } = require("./m")` => named bindings
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name) = parent.child_by_field_name("name") {
                return js_require_binding(source, name, content);
            }
        }

        if parent.kind() == "assignment_expression" {
            if let Some(left) = parent.child_by_field_name("left") {
                if left.kind() == "identifier" {
                    return vec![Import::new(
                        source,
                        Some("*".to_string()),
                        node_text(left, content),
                        false,
                    )];
                }
            }
        }
    }

    vec![Import::new(source, None, None, false)]
}

fn js_require_binding(source: String, name: tree_sitter::Node, content: &str) -> Vec<Import> {
    match name.kind() {
        "identifier" => vec![Import::new(
            source,
            Some("*".to_string()),
            node_text(name, content),
            false,
        )],
        "object_pattern" => {
            let mut imports = Vec::new();
            let mut cursor = name.walk();
            for child in name.children(&mut cursor) {
                match child.kind() {
                    "shorthand_property_identifier_pattern" => {
                        if let Some(sym) = node_text(child, content) {
                            imports.push(Import::new(source.clone(), Some(sym), None, false));
                        }
                    }
                    "pair_pattern" => {
                        let key = child
                            .child_by_field_name("key")
                            .and_then(|n| node_text(n, content))
                            .or_else(|| {
                                let mut pc = child.walk();
                                let first = child.children(&mut pc).next();
                                first.and_then(|n| node_text(n, content))
                            });
                        let value = child
                            .child_by_field_name("value")
                            .and_then(|n| node_text(n, content));
                        if let Some(key) = key {
                            imports.push(Import::new(source.clone(), Some(key), value, false));
                        }
                    }
                    _ => {}
                }
            }
            imports
        }
        _ => vec![Import::new(source, None, None, false)],
    }
}

// --- Python ---

fn python_import(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    match node.kind() {
        "import_statement" => python_import_statement(node, content),
        "import_from_statement" => python_import_from_statement(node, content),
        _ => Vec::new(),
    }
}

fn python_import_statement(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    let mut imports = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "aliased_import" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| node_text(n, content));
                let alias = child
                    .child_by_field_name("alias")
                    .and_then(|n| node_text(n, content));
                if let Some(name) = name {
                    let local = alias
                        .clone()
                        .or_else(|| name.split('.').next().map(|s| s.to_string()));
                    imports.push(Import::new(name, None, local, false));
                }
            }
            "dotted_name" => {
                if let Some(name) = node_text(child, content) {
                    let local = name.split('.').next().map(|s| s.to_string());
                    imports.push(Import::new(name, None, local, false));
                }
            }
            _ => {}
        }
    }
    imports
}

fn python_import_from_statement(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    let module = node
        .child_by_field_name("module_name")
        .and_then(|n| node_text(n, content))
        .or_else(|| {
            let mut cursor = node.walk();
            let found = node
                .children(&mut cursor)
                .find(|c| c.kind() == "dotted_name")
                .and_then(|n| node_text(n, content));
            found
        })
        .unwrap_or_default();

    let mut imports = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "wildcard_import" => {
                imports.push(Import::new(
                    module.clone(),
                    Some("*".to_string()),
                    None,
                    false,
                ));
            }
            "aliased_import" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| node_text(n, content))
                    .or_else(|| node_text(child, content));
                let alias = child
                    .child_by_field_name("alias")
                    .and_then(|n| node_text(n, content));
                if let Some(name) = name {
                    imports.push(Import::new(module.clone(), Some(name), alias, false));
                }
            }
            "identifier" => {
                if let Some(name) = node_text(child, content) {
                    if name != "from" && name != "import" {
                        imports.push(Import::new(module.clone(), Some(name), None, false));
                    }
                }
            }
            _ => {}
        }
    }

    imports
}

// --- Rust ---

fn rust_import(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    if node.kind() != "use_declaration" {
        return Vec::new();
    }

    let text = match node_text(node, content) {
        Some(text) => text,
        None => return Vec::new(),
    };
    let text = text.trim();
    let text = text.strip_prefix("use ").unwrap_or(text);
    let text = text.trim_end_matches(';').trim();
    parse_rust_use(text)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RustTok {
    Ident(String),
    ColonColon,
    LBrace,
    RBrace,
    Comma,
    Star,
    As,
}

fn parse_rust_use(text: &str) -> Vec<Import> {
    let tokens = rust_tokenize(text);
    let mut idx = 0usize;
    parse_rust_use_tree(&tokens, &mut idx, Vec::new())
}

fn rust_tokenize(text: &str) -> Vec<RustTok> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => i += 1,
            ':' if i + 1 < chars.len() && chars[i + 1] == ':' => {
                tokens.push(RustTok::ColonColon);
                i += 2;
            }
            '{' => {
                tokens.push(RustTok::LBrace);
                i += 1;
            }
            '}' => {
                tokens.push(RustTok::RBrace);
                i += 1;
            }
            ',' => {
                tokens.push(RustTok::Comma);
                i += 1;
            }
            '*' => {
                tokens.push(RustTok::Star);
                i += 1;
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
                {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                if ident == "as" {
                    tokens.push(RustTok::As);
                } else if !ident.is_empty() {
                    tokens.push(RustTok::Ident(ident));
                } else {
                    i += 1;
                }
            }
        }
    }

    tokens
}

fn parse_rust_use_tree(tokens: &[RustTok], idx: &mut usize, prefix: Vec<String>) -> Vec<Import> {
    if *idx >= tokens.len() {
        return Vec::new();
    }

    match tokens.get(*idx) {
        Some(RustTok::LBrace) => {
            *idx += 1;
            let mut out = Vec::new();
            loop {
                if *idx >= tokens.len() {
                    break;
                }
                if matches!(tokens.get(*idx), Some(RustTok::RBrace)) {
                    *idx += 1;
                    break;
                }
                out.extend(parse_rust_use_tree(tokens, idx, prefix.clone()));
                match tokens.get(*idx) {
                    Some(RustTok::Comma) => *idx += 1,
                    Some(RustTok::RBrace) => {}
                    _ => {}
                }
            }
            out
        }
        _ => {
            let mut path = Vec::new();
            while *idx < tokens.len() {
                match tokens.get(*idx) {
                    Some(RustTok::Ident(ident)) => {
                        path.push(ident.clone());
                        *idx += 1;
                    }
                    Some(RustTok::Star) => {
                        path.push("*".to_string());
                        *idx += 1;
                    }
                    _ => break,
                }

                if matches!(tokens.get(*idx), Some(RustTok::ColonColon)) {
                    *idx += 1;
                    continue;
                }
                break;
            }

            if matches!(tokens.get(*idx), Some(RustTok::LBrace)) {
                let mut new_prefix = prefix;
                new_prefix.extend(path);
                return parse_rust_use_tree(tokens, idx, new_prefix);
            }

            let alias = if matches!(tokens.get(*idx), Some(RustTok::As)) {
                *idx += 1;
                match tokens.get(*idx) {
                    Some(RustTok::Ident(name)) => {
                        *idx += 1;
                        Some(name.clone())
                    }
                    _ => None,
                }
            } else {
                None
            };

            let mut full = prefix;
            full.extend(path);

            if full.is_empty() {
                return Vec::new();
            }

            if full.len() == 1 {
                return vec![Import::new(full[0].clone(), None, alias, false)];
            }

            let symbol = full.pop().unwrap();
            let source = full.join("::");
            vec![Import::new(source, Some(symbol), alias, false)]
        }
    }
}

// --- Java ---

fn java_import(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    if node.kind() != "import_declaration" {
        return Vec::new();
    }

    let text = node_text(node, content).unwrap_or_default();
    let text = text.trim().trim_end_matches(';').trim();
    let text = text.strip_prefix("import ").unwrap_or(text).trim();
    let text = text.strip_prefix("static ").unwrap_or(text).trim();

    if text.ends_with(".*") {
        let source = text.trim_end_matches(".*").to_string();
        return vec![Import::new(source, Some("*".to_string()), None, false)];
    }

    if let Some((source, symbol)) = text.rsplit_once('.') {
        return vec![Import::new(
            source.to_string(),
            Some(symbol.to_string()),
            None,
            false,
        )];
    }

    vec![Import::new(text.to_string(), None, None, false)]
}

// --- Go ---

fn go_import(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    if node.kind() != "import_spec" {
        return Vec::new();
    }

    let mut cursor = node.walk();
    let source = node
        .children(&mut cursor)
        .find(|c| c.kind().contains("string_literal"))
        .and_then(|n| node_text(n, content))
        .map(|s| unquote(&s));

    let Some(source) = source else {
        return Vec::new();
    };

    let mut cursor = node.walk();
    let alias = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")
        .and_then(|n| node_text(n, content))
        .or_else(|| source.split('/').next_back().map(|s| s.to_string()));

    vec![Import::new(source, None, alias, false)]
}

// --- C / C++ ---

fn c_import(node: tree_sitter::Node, content: &str) -> Vec<Import> {
    if node.kind() != "preproc_include" {
        return Vec::new();
    }

    let mut cursor = node.walk();
    let header = node
        .children(&mut cursor)
        .find(|c| c.kind() == "string_literal" || c.kind() == "system_lib_string")
        .and_then(|n| node_text(n, content))
        .map(|s| unquote(&s));

    header
        .map(|h| vec![Import::new(h, None, None, false)])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_root(content: &str, language: Language) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language.tree_sitter_language().unwrap())
            .unwrap();
        parser.parse(content, None).unwrap()
    }

    #[test]
    fn test_extract_imports_typescript_named_imports() {
        let content = r#"import { foo as bar, baz } from "./m";"#;
        let tree = parse_root(content, Language::TypeScript);
        let imports = extract_imports(tree.root_node(), content, Language::TypeScript);

        assert!(imports.iter().any(|i| {
            i.source == "./m"
                && i.symbol.as_deref() == Some("foo")
                && i.alias.as_deref() == Some("bar")
        }));
        assert!(imports
            .iter()
            .any(|i| i.source == "./m" && i.symbol.as_deref() == Some("baz")));
    }

    #[test]
    fn test_extract_imports_python_from_import() {
        let content = "from pkg.mod import func as f\n";
        let tree = parse_root(content, Language::Python);
        let imports = extract_imports(tree.root_node(), content, Language::Python);

        assert!(imports.iter().any(|i| {
            i.source == "pkg.mod"
                && i.symbol.as_deref() == Some("func")
                && i.alias.as_deref() == Some("f")
        }));
    }
}
