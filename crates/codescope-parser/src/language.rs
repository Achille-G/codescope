//! Language detection and grammar loading

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    TypeScript,
    JavaScript,
    Tsx,
    Jsx,
    Python,
    Rust,
    Java,
    C,
    Cpp,
    Go,
    Html,
    Css,
    Scss,
    Json,
    Yaml,
}

impl Language {
    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "jsx" => Some(Language::Jsx),
            "py" | "pyi" => Some(Language::Python),
            "rs" => Some(Language::Rust),
            "java" => Some(Language::Java),
            "c" | "h" => Some(Language::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some(Language::Cpp),
            "go" => Some(Language::Go),
            "html" | "htm" => Some(Language::Html),
            "css" => Some(Language::Css),
            "scss" | "sass" => Some(Language::Scss),
            "json" => Some(Language::Json),
            "yaml" | "yml" => Some(Language::Yaml),
            _ => None,
        }
    }

    /// Detect language from file path
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    /// Get the tree-sitter language for this language
    pub fn tree_sitter_language(&self) -> Result<tree_sitter::Language> {
        let lang = match self {
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
            Language::JavaScript | Language::Jsx => tree_sitter_javascript::LANGUAGE,
            Language::Python => tree_sitter_python::LANGUAGE,
            Language::Rust => tree_sitter_rust::LANGUAGE,
            Language::Java => tree_sitter_java::LANGUAGE,
            Language::C => tree_sitter_c::LANGUAGE,
            Language::Cpp => tree_sitter_cpp::LANGUAGE,
            Language::Go => tree_sitter_go::LANGUAGE,
            Language::Html => tree_sitter_html::LANGUAGE,
            Language::Css | Language::Scss => tree_sitter_css::LANGUAGE,
            Language::Json => tree_sitter_json::LANGUAGE,
            Language::Yaml => {
                return Err(Error::UnsupportedLanguage("yaml".to_string()));
            }
        };
        Ok(lang.into())
    }

    /// Get the language name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Tsx => "tsx",
            Language::Jsx => "jsx",
            Language::Python => "python",
            Language::Rust => "rust",
            Language::Java => "java",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Go => "go",
            Language::Html => "html",
            Language::Css => "css",
            Language::Scss => "scss",
            Language::Json => "json",
            Language::Yaml => "yaml",
        }
    }

    /// Check if this language supports AST-based chunking
    pub fn supports_ast_chunking(&self) -> bool {
        !matches!(
            self,
            Language::Json | Language::Yaml | Language::Css | Language::Scss
        )
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::Tsx));
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("unknown"), None);
    }

    #[test]
    fn test_language_from_path() {
        let path = Path::new("src/main.rs");
        assert_eq!(Language::from_path(path), Some(Language::Rust));

        let path = Path::new("components/Button.tsx");
        assert_eq!(Language::from_path(path), Some(Language::Tsx));
    }
}
