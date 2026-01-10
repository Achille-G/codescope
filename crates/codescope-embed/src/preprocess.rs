//! Text preprocessing for code embeddings

/// Preprocess code text for embedding
///
/// This includes:
/// - Splitting camelCase and snake_case identifiers
/// - Normalizing whitespace
/// - Truncating to max length
pub fn preprocess_code(text: &str, max_chars: usize) -> String {
    let mut result = String::with_capacity(text.len());

    // Split identifiers and normalize
    let mut prev_was_lower = false;
    let mut prev_was_underscore = false;

    for ch in text.chars().take(max_chars) {
        // Handle camelCase: insert space before uppercase if previous was lowercase
        if ch.is_uppercase() && prev_was_lower {
            result.push(' ');
        }

        // Handle snake_case: replace underscore with space
        if ch == '_' {
            if !prev_was_underscore && !result.ends_with(' ') {
                result.push(' ');
            }
            prev_was_underscore = true;
            continue;
        }

        // Normalize multiple whitespace to single space
        if ch.is_whitespace() {
            if !result.ends_with(' ') && !result.is_empty() {
                result.push(' ');
            }
            prev_was_lower = false;
            prev_was_underscore = false;
            continue;
        }

        result.push(ch);
        prev_was_lower = ch.is_lowercase();
        prev_was_underscore = false;
    }

    result.trim().to_string()
}

/// Preprocess a batch of texts
pub fn preprocess_batch(texts: &[&str], max_chars: usize) -> Vec<String> {
    texts
        .iter()
        .map(|t| preprocess_code(t, max_chars))
        .collect()
}

/// Estimate token count (rough approximation)
///
/// This is a simple heuristic: ~4 characters per token on average
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_case_split() {
        assert_eq!(preprocess_code("getUserName", 1000), "get User Name");
        assert_eq!(preprocess_code("XMLParser", 1000), "X M L Parser");
    }

    #[test]
    fn test_snake_case_split() {
        assert_eq!(preprocess_code("get_user_name", 1000), "get user name");
        assert_eq!(preprocess_code("__init__", 1000), "init");
    }

    #[test]
    fn test_whitespace_normalization() {
        assert_eq!(preprocess_code("hello   world", 1000), "hello world");
        assert_eq!(preprocess_code("hello\n\n\tworld", 1000), "hello world");
    }

    #[test]
    fn test_truncation() {
        let long_text = "a".repeat(1000);
        let result = preprocess_code(&long_text, 100);
        assert_eq!(result.len(), 100);
    }

    #[test]
    fn test_mixed_case() {
        assert_eq!(
            preprocess_code("getUserName_from_API", 1000),
            "get User Name from A P I"
        );
    }
}
