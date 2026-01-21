//! Call site extraction from Tree-sitter AST nodes.

use crate::Language;
use serde::{Deserialize, Serialize};

/// A call site found within a chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    /// The called symbol name (best-effort).
    pub callee_name: String,
    /// Call expression start line (1-indexed).
    pub line: u32,
    /// Call expression start column (1-indexed).
    pub column: Option<u32>,
    /// Whether this call looks like a method/selector call.
    pub is_method: bool,
    /// Receiver expression for method/selector calls (best-effort).
    pub receiver: Option<String>,
}

impl CallSite {
    pub fn new(
        callee_name: String,
        line: u32,
        column: Option<u32>,
        is_method: bool,
        receiver: Option<String>,
    ) -> Self {
        Self {
            callee_name,
            line,
            column,
            is_method,
            receiver,
        }
    }
}

/// Extract call sites from the provided node.
pub fn extract_call_sites(
    node: tree_sitter::Node,
    content: &str,
    language: Language,
) -> Vec<CallSite> {
    match language {
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
            collect(node, |n| js_call_site(n, content))
        }
        Language::Python => collect(node, |n| python_call_site(n, content)),
        Language::Rust => collect(node, |n| rust_call_site(n, content)),
        Language::Java => collect(node, |n| java_call_site(n, content)),
        Language::Go => collect(node, |n| go_call_site(n, content)),
        Language::C | Language::Cpp => collect(node, |n| c_call_site(n, content)),
        _ => Vec::new(),
    }
}

/// Extract a single call site from `node` if `node` represents a call expression.
pub fn extract_call_site(
    node: tree_sitter::Node,
    content: &str,
    language: Language,
) -> Option<CallSite> {
    match language {
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
            js_call_site(node, content)
        }
        Language::Python => python_call_site(node, content),
        Language::Rust => rust_call_site(node, content),
        Language::Java => java_call_site(node, content),
        Language::Go => go_call_site(node, content),
        Language::C | Language::Cpp => c_call_site(node, content),
        _ => None,
    }
}

fn collect(
    node: tree_sitter::Node,
    mut parse: impl FnMut(tree_sitter::Node) -> Option<CallSite>,
) -> Vec<CallSite> {
    let mut stack = vec![node];
    let mut out = Vec::new();

    while let Some(current) = stack.pop() {
        if let Some(site) = parse(current) {
            out.push(site);
        }

        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }

    out
}

fn point_to_location(point: tree_sitter::Point) -> (u32, Option<u32>) {
    let line = u32::try_from(point.row)
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    let column = u32::try_from(point.column)
        .ok()
        .map(|c| c.saturating_add(1));
    (line, column)
}

fn node_text(node: tree_sitter::Node, content: &str) -> Option<String> {
    node.utf8_text(content.as_bytes())
        .ok()
        .map(|s| s.to_string())
}

fn unwrap_wrappers<'a>(mut node: tree_sitter::Node<'a>, field: &str) -> tree_sitter::Node<'a> {
    loop {
        match node.kind() {
            "parenthesized_expression" | "await_expression" => {
                if let Some(inner) = node.named_child(0) {
                    node = inner;
                    continue;
                }
            }
            "optional_chain" => {
                if let Some(inner) = node
                    .child_by_field_name(field)
                    .or_else(|| node.named_child(0))
                {
                    node = inner;
                    continue;
                }
            }
            _ => {}
        }
        break;
    }
    node
}

fn split_scoped(text: &str) -> (String, bool, Option<String>) {
    let mut parts: Vec<&str> = text.split("::").filter(|p| !p.is_empty()).collect();
    let callee_name = parts.pop().unwrap_or(text).to_string();
    let receiver = if parts.is_empty() {
        None
    } else {
        Some(parts.join("::"))
    };
    (callee_name, false, receiver)
}

fn js_call_site(node: tree_sitter::Node, content: &str) -> Option<CallSite> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function = node.child_by_field_name("function")?;
    let function = unwrap_wrappers(function, "expression");
    let (callee_name, is_method, receiver) = js_callee(function, content)?;

    // Treat module loading as an import, not a call graph edge.
    if !is_method && receiver.is_none() && callee_name == "require" {
        return None;
    }

    let (line, column) = point_to_location(node.start_position());
    Some(CallSite::new(
        callee_name,
        line,
        column,
        is_method,
        receiver,
    ))
}

fn js_callee(node: tree_sitter::Node, content: &str) -> Option<(String, bool, Option<String>)> {
    match node.kind() {
        "identifier" => Some((node_text(node, content)?, false, None)),
        "member_expression" => {
            let property = node.child_by_field_name("property")?;
            let object = node.child_by_field_name("object")?;
            Some((
                node_text(property, content)?,
                true,
                node_text(object, content),
            ))
        }
        _ => None,
    }
}

fn python_call_site(node: tree_sitter::Node, content: &str) -> Option<CallSite> {
    if node.kind() != "call" {
        return None;
    }

    let function = node.child_by_field_name("function")?;
    let (callee_name, is_method, receiver) = match function.kind() {
        "identifier" => (node_text(function, content)?, false, None),
        "attribute" => {
            let attribute = function.child_by_field_name("attribute")?;
            let object = function.child_by_field_name("object")?;
            (
                node_text(attribute, content)?,
                true,
                node_text(object, content),
            )
        }
        _ => return None,
    };

    let (line, column) = point_to_location(node.start_position());
    Some(CallSite::new(
        callee_name,
        line,
        column,
        is_method,
        receiver,
    ))
}

fn rust_call_site(node: tree_sitter::Node, content: &str) -> Option<CallSite> {
    match node.kind() {
        "call_expression" => {
            let function = node.child_by_field_name("function")?;
            let function = unwrap_wrappers(function, "expression");
            let (callee_name, is_method, receiver) = match function.kind() {
                "identifier" => (node_text(function, content)?, false, None),
                "scoped_identifier" | "scoped_type_identifier" => {
                    let text = node_text(function, content)?;
                    split_scoped(&text)
                }
                "field_expression" => {
                    let field = function.child_by_field_name("field")?;
                    let value = function.child_by_field_name("value")?;
                    (node_text(field, content)?, true, node_text(value, content))
                }
                _ => return None,
            };

            let (line, column) = point_to_location(node.start_position());
            Some(CallSite::new(
                callee_name,
                line,
                column,
                is_method,
                receiver,
            ))
        }
        "method_call_expression" => {
            let receiver_node = node.child_by_field_name("receiver")?;
            let name_node = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("method"))?;
            let callee_name = node_text(name_node, content)?;
            let receiver = node_text(receiver_node, content);
            let (line, column) = point_to_location(node.start_position());
            Some(CallSite::new(callee_name, line, column, true, receiver))
        }
        _ => None,
    }
}

fn java_call_site(node: tree_sitter::Node, content: &str) -> Option<CallSite> {
    if node.kind() != "method_invocation" {
        return None;
    }

    let name_node = node.child_by_field_name("name")?;
    let callee_name = node_text(name_node, content)?;
    let receiver = node
        .child_by_field_name("object")
        .and_then(|n| node_text(n, content));
    let is_method = receiver.is_some();

    let (line, column) = point_to_location(node.start_position());
    Some(CallSite::new(
        callee_name,
        line,
        column,
        is_method,
        receiver,
    ))
}

fn go_call_site(node: tree_sitter::Node, content: &str) -> Option<CallSite> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function = node.child_by_field_name("function")?;
    let (callee_name, is_method, receiver) = match function.kind() {
        "identifier" => (node_text(function, content)?, false, None),
        "selector_expression" => {
            let field = function
                .child_by_field_name("field")
                .or_else(|| function.child_by_field_name("name"))?;
            let operand = function
                .child_by_field_name("operand")
                .or_else(|| function.child_by_field_name("object"))?;
            (
                node_text(field, content)?,
                true,
                node_text(operand, content),
            )
        }
        _ => return None,
    };

    let (line, column) = point_to_location(node.start_position());
    Some(CallSite::new(
        callee_name,
        line,
        column,
        is_method,
        receiver,
    ))
}

fn c_call_site(node: tree_sitter::Node, content: &str) -> Option<CallSite> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function = node.child_by_field_name("function")?;
    let function = unwrap_wrappers(function, "expression");
    let (callee_name, is_method, receiver) = match function.kind() {
        "identifier" => (node_text(function, content)?, false, None),
        "scoped_identifier" => {
            let text = node_text(function, content)?;
            split_scoped(&text)
        }
        "field_expression" => {
            let field = function.child_by_field_name("field")?;
            let value = function.child_by_field_name("argument")?;
            (node_text(field, content)?, true, node_text(value, content))
        }
        _ => return None,
    };

    let (line, column) = point_to_location(node.start_position());
    Some(CallSite::new(
        callee_name,
        line,
        column,
        is_method,
        receiver,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Language;

    fn parse_root(content: &str, language: Language) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language.tree_sitter_language().unwrap())
            .unwrap();
        parser.parse(content, None).unwrap()
    }

    #[test]
    fn test_extract_call_sites_javascript_identifier() {
        let content = "function a() { b(); }";
        let tree = parse_root(content, Language::JavaScript);
        let sites = extract_call_sites(tree.root_node(), content, Language::JavaScript);
        assert!(sites.iter().any(|s| s.callee_name == "b" && !s.is_method));
    }

    #[test]
    fn test_extract_call_sites_python_attribute() {
        let content = "def a():\n    os.path.join('a','b')\n";
        let tree = parse_root(content, Language::Python);
        let sites = extract_call_sites(tree.root_node(), content, Language::Python);
        assert!(sites.iter().any(|s| s.callee_name == "join" && s.is_method));
    }

    #[test]
    fn test_extract_call_sites_rust_method_call() {
        let content = "fn a() { foo.bar(); }";
        let tree = parse_root(content, Language::Rust);
        let sites = extract_call_sites(tree.root_node(), content, Language::Rust);
        assert!(sites.iter().any(|s| s.callee_name == "bar" && s.is_method));
    }
}
