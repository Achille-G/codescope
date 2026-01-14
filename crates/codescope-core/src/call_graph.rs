//! Call graph tracing and formatting utilities.

use crate::{Error, Result};
use codescope_search::storage::{CallTargetKind, CalleeInfo, CallerInfo, ChunkRecord};
use codescope_search::Storage;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct CallGraphNode {
    pub id: usize,
    pub symbol: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub depth: usize,
    pub target_kind: CallTargetKind,
}

#[derive(Debug, Clone)]
pub struct CallGraphEdge {
    pub from: usize,
    pub to: usize,
    pub call_line: u32,
    pub call_column: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CallGraph {
    pub nodes: Vec<CallGraphNode>,
    pub edges: Vec<CallGraphEdge>,
}

struct NodeProps {
    symbol: String,
    file: Option<String>,
    line: Option<u32>,
    depth: usize,
    target_kind: CallTargetKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NodeKey {
    Resolved(i64),
    Unresolved {
        symbol: String,
        kind: CallTargetKind,
    },
}

impl CallGraph {
    pub fn callers(storage: &Storage, symbol: &str, file: Option<&str>) -> Result<Vec<CallerInfo>> {
        if let Some(file) = file {
            let chunks = storage.find_chunks_by_symbol_in_file(symbol, file)?;
            if chunks.is_empty() {
                return Err(Error::CallGraph(format!(
                    "Symbol not found: {symbol} (file={file})"
                )));
            }
            let ids: Vec<i64> = chunks.iter().map(|c| c.chunk_id).collect();
            return Ok(storage.get_callers_for_resolved_chunk_ids(&ids)?);
        }

        Ok(storage.get_callers(symbol)?)
    }

    pub fn callees(
        storage: &Storage,
        symbol: &str,
        file: Option<&str>,
    ) -> Result<(ChunkRecord, Vec<CalleeInfo>)> {
        let root = select_unique_chunk(storage, symbol, file)?;
        let callees = storage.get_callees(root.chunk_id)?;
        Ok((root, callees))
    }

    pub fn build(
        storage: &Storage,
        symbol: &str,
        file: Option<&str>,
        max_depth: usize,
    ) -> Result<Self> {
        let root = select_unique_chunk(storage, symbol, file)?;
        let root_symbol = root.symbol.clone().unwrap_or_else(|| symbol.to_string());

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let mut node_ids: HashMap<NodeKey, usize> = HashMap::new();
        let mut queue: VecDeque<(i64, usize, usize)> = VecDeque::new();
        let mut expanded: HashSet<(i64, usize)> = HashSet::new();

        let root_id = push_node(
            &mut nodes,
            &mut node_ids,
            NodeKey::Resolved(root.chunk_id),
            NodeProps {
                symbol: root_symbol,
                file: Some(root.file_path.clone()),
                line: Some(root.start_line),
                depth: 0,
                target_kind: CallTargetKind::Project,
            },
        );
        queue.push_back((root.chunk_id, root_id, 0));

        while let Some((chunk_id, from_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            if !expanded.insert((chunk_id, depth)) {
                continue;
            }

            let callees = storage.get_callees(chunk_id)?;
            for callee in callees {
                let next_depth = depth + 1;
                let (to_key, to_symbol, to_file, to_line, resolved_chunk_id, target_kind) =
                    callee_to_node_key(&callee);

                let to_id = push_node(
                    &mut nodes,
                    &mut node_ids,
                    to_key,
                    NodeProps {
                        symbol: to_symbol,
                        file: to_file,
                        line: to_line,
                        depth: next_depth,
                        target_kind,
                    },
                );

                edges.push(CallGraphEdge {
                    from: from_id,
                    to: to_id,
                    call_line: callee.call_line,
                    call_column: callee.call_column,
                });

                if let Some(resolved_chunk_id) = resolved_chunk_id {
                    queue.push_back((resolved_chunk_id, to_id, next_depth));
                }
            }
        }

        Ok(Self { nodes, edges })
    }

    pub fn to_jsonl(&self) -> Result<String> {
        let mut lines = Vec::new();

        for node in &self.nodes {
            lines.push(
                json!({
                    "type": "node",
                    "id": node.id,
                    "symbol": node.symbol,
                    "file": node.file,
                    "line": node.line,
                    "depth": node.depth,
                    "target_kind": node.target_kind.as_str()
                })
                .to_string(),
            );
        }

        for edge in &self.edges {
            lines.push(
                json!({
                    "type": "edge",
                    "from": edge.from,
                    "to": edge.to,
                    "call_line": edge.call_line,
                    "call_column": edge.call_column
                })
                .to_string(),
            );
        }

        Ok(lines.join("\n"))
    }

    pub fn to_dot(&self) -> String {
        let mut out = String::new();
        out.push_str("digraph call_graph {\n");
        out.push_str("    rankdir=LR;\n");
        out.push_str("    node [shape=box];\n\n");

        let mut id_to_label: HashMap<usize, String> = HashMap::new();
        for node in &self.nodes {
            let label = match (&node.file, node.line) {
                (Some(file), Some(line)) => format!("{}\\n{}:{}", node.symbol, file, line),
                _ => format!("{}\\n({})", node.symbol, node.target_kind),
            };
            id_to_label.insert(node.id, label);
        }

        for edge in &self.edges {
            let from = id_to_label.get(&edge.from).cloned().unwrap_or_default();
            let to = id_to_label.get(&edge.to).cloned().unwrap_or_default();
            out.push_str(&format!("    \"{from}\" -> \"{to}\";\n"));
        }

        out.push_str("}\n");
        out
    }
}

fn select_unique_chunk(storage: &Storage, symbol: &str, file: Option<&str>) -> Result<ChunkRecord> {
    let candidates = if let Some(file) = file {
        storage.find_chunks_by_symbol_in_file(symbol, file)?
    } else {
        storage.find_chunks_by_symbol(symbol)?
    };

    if candidates.is_empty() {
        return Err(Error::CallGraph(format!("Symbol not found: {symbol}")));
    }

    if candidates.len() == 1 {
        return Ok(candidates[0].clone());
    }

    let mut msg = format!("Ambiguous symbol: {symbol}\nCandidates:\n");
    for c in &candidates {
        msg.push_str(&format!(
            "- {}:{} (kind={})\n",
            c.file_path, c.start_line, c.kind
        ));
    }
    msg.push_str("Use --file to disambiguate.");
    Err(Error::CallGraph(msg))
}

fn push_node(
    nodes: &mut Vec<CallGraphNode>,
    node_ids: &mut HashMap<NodeKey, usize>,
    key: NodeKey,
    props: NodeProps,
) -> usize {
    if let Some(id) = node_ids.get(&key).copied() {
        return id;
    }

    let id = nodes.len() + 1;
    nodes.push(CallGraphNode {
        id,
        symbol: props.symbol,
        file: props.file,
        line: props.line,
        depth: props.depth,
        target_kind: props.target_kind,
    });
    node_ids.insert(key, id);
    id
}

fn callee_to_node_key(
    callee: &CalleeInfo,
) -> (
    NodeKey,
    String,
    Option<String>,
    Option<u32>,
    Option<i64>,
    CallTargetKind,
) {
    if let Some(chunk_id) = callee.resolved_chunk_id {
        let symbol = callee
            .resolved_symbol
            .clone()
            .unwrap_or_else(|| callee.callee_name.clone());
        return (
            NodeKey::Resolved(chunk_id),
            symbol,
            callee.resolved_file.clone(),
            callee.resolved_line,
            Some(chunk_id),
            CallTargetKind::Project,
        );
    }

    let symbol = if let Some(receiver) = callee.receiver.as_deref() {
        format!("{receiver}.{}", callee.callee_name)
    } else {
        callee.callee_name.clone()
    };

    (
        NodeKey::Unresolved {
            symbol: symbol.clone(),
            kind: callee.target_kind,
        },
        symbol,
        None,
        None,
        None,
        callee.target_kind,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_call_graph_jsonl_and_dot() {
        let storage = Storage::open_memory().unwrap();

        let file_id_b = storage
            .upsert_file("src/b.ts", Some("typescript"), b"h_b", 10)
            .unwrap();
        storage
            .insert_chunk(
                file_id_b,
                Some("foo"),
                "function",
                1,
                1,
                b"c_b",
                "export function foo() {}",
            )
            .unwrap();

        let file_id_a = storage
            .upsert_file("src/a.ts", Some("typescript"), b"h_a", 10)
            .unwrap();
        storage
            .insert_import(file_id_a, "./b", Some("foo"), None, false)
            .unwrap();

        let caller_chunk_id = storage
            .insert_chunk(
                file_id_a,
                Some("caller"),
                "function",
                1,
                3,
                b"c_a",
                "import { foo } from './b';\nexport function caller() { foo(); }\n",
            )
            .unwrap();
        storage
            .insert_call_site(caller_chunk_id, "foo", 2, Some(1), false, None)
            .unwrap();
        storage.resolve_call_sites(file_id_a).unwrap();

        let graph = CallGraph::build(&storage, "caller", Some("src/a.ts"), 2).unwrap();
        assert!(graph.nodes.iter().any(|n| n.symbol == "caller"));
        assert!(graph.nodes.iter().any(|n| n.symbol == "foo"));
        assert_eq!(graph.edges.len(), 1);

        let jsonl = graph.to_jsonl().unwrap();
        assert!(jsonl.contains("\"type\":\"node\""));
        assert!(jsonl.contains("\"type\":\"edge\""));

        let dot = graph.to_dot();
        assert!(dot.contains("digraph call_graph"));
    }
}
