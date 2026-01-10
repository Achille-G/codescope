//! Light, heuristic reranking for code search results.

use crate::SearchResult;

/// Apply heuristic reranking in-place (updates scores and order).
pub fn rerank(query: &str, results: &mut Vec<SearchResult>) {
    if results.is_empty() {
        return;
    }

    let query_norm = normalize_token(query);
    let query_lower = query.to_lowercase();

    // 1) Symbol exact/near match boost.
    for result in results.iter_mut() {
        let Some(symbol) = result.symbol.as_deref() else {
            continue;
        };

        let symbol_norm = normalize_token(symbol);
        let symbol_lower = symbol.to_lowercase();

        if !query_norm.is_empty() && query_norm == symbol_norm {
            result.score *= 1.25;
            continue;
        }

        if !query_norm.is_empty()
            && !symbol_norm.is_empty()
            && (query_norm.contains(&symbol_norm) || symbol_norm.contains(&query_norm))
        {
            result.score *= 1.10;
        } else if query_lower.contains(&symbol_lower) || symbol_lower.contains(&query_lower) {
            result.score *= 1.08;
        }
    }

    // 2) File proximity boost: prefer multiple hits in the same file as the top hit.
    if let Some(top) = results.first() {
        let top_file = top.file.clone();
        for result in results.iter_mut().skip(1) {
            if result.file == top_file {
                result.score *= 1.05;
            }
        }
    }

    // Stable-ish sort: prefer higher score, then earlier original order.
    let mut with_idx: Vec<(usize, f32)> = results
        .iter()
        .enumerate()
        .map(|(idx, r)| (idx, r.score))
        .collect();
    with_idx.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut reordered = Vec::with_capacity(results.len());
    for (idx, _) in with_idx {
        reordered.push(results[idx].clone());
    }
    *results = reordered;
}

fn normalize_token(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(file: &str, symbol: Option<&str>, score: f32) -> SearchResult {
        let mut res = SearchResult::new(
            file.to_string(),
            symbol.map(|s| s.to_string()),
            "function".to_string(),
            1,
            1,
            score,
            "x".to_string(),
        );
        res.chunk_id = Some(1);
        res
    }

    #[test]
    fn test_symbol_exact_match_boosts() {
        let mut results = vec![
            r("src/a.rs", Some("other"), 1.0),
            r("src/b.rs", Some("hello_world"), 1.0),
        ];

        rerank("hello_world", &mut results);
        assert_eq!(results[0].symbol.as_deref(), Some("hello_world"));
    }

    #[test]
    fn test_file_proximity_boosts_same_file() {
        let mut results = vec![
            r("src/a.rs", Some("first"), 1.0),
            r("src/a.rs", Some("second"), 1.0),
            r("src/b.rs", Some("third"), 1.0),
        ];

        rerank("anything", &mut results);
        assert_eq!(results[0].file, "src/a.rs");
        assert_eq!(results[1].file, "src/a.rs");
    }
}
