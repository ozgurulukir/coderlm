use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Java,
    Scala,
    C,
    Cpp,
    Ruby,
    Shell,
    Markdown,
    Json,
    Yaml,
    Toml,
    Html,
    Css,
    Sql,
    Vue,
    Other,
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" | "pyi" => Language::Python,
            "ts" | "tsx" => Language::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "go" => Language::Go,
            "java" => Language::Java,
            "scala" | "sc" => Language::Scala,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Language::Cpp,
            "rb" => Language::Ruby,
            "sh" | "bash" | "zsh" | "fish" => Language::Shell,
            "md" | "mdx" => Language::Markdown,
            "json" | "jsonc" => Language::Json,
            "yml" | "yaml" => Language::Yaml,
            "toml" => Language::Toml,
            "html" | "htm" => Language::Html,
            "css" | "scss" | "less" => Language::Css,
            "sql" => Language::Sql,
            "vue" => Language::Vue,
            _ => Language::Other,
        }
    }

    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map(Self::from_extension)
            .unwrap_or(Language::Other)
    }

    /// Whether this language supports tree-sitter symbol extraction.
    pub fn has_tree_sitter_support(&self) -> bool {
        matches!(
            self,
            Language::Rust | Language::Python | Language::TypeScript | Language::JavaScript | Language::Go
            | Language::Java | Language::Scala | Language::Vue
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileMark {
    Documentation,
    Ignore,
    Test,
    Config,
    Generated,
    Custom,
}

impl FileMark {
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "documentation" | "doc" | "docs" => Some(FileMark::Documentation),
            "ignore" => Some(FileMark::Ignore),
            "test" | "tests" => Some(FileMark::Test),
            "config" | "configuration" => Some(FileMark::Config),
            "generated" | "gen" => Some(FileMark::Generated),
            "custom" => Some(FileMark::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub rel_path: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub language: Language,
    /// Agent-set human-readable definition of what this file does.
    pub definition: Option<String>,
    /// Agent-set marks for categorization.
    pub marks: Vec<FileMark>,
    /// Whether symbols have been extracted from this file.
    pub symbols_extracted: bool,
}

impl FileEntry {
    pub fn new(rel_path: String, size: u64, modified: DateTime<Utc>) -> Self {
        let language = Language::from_path(Path::new(&rel_path));
        Self {
            rel_path,
            size,
            modified,
            language,
            definition: None,
            marks: Vec::new(),
            symbols_extracted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension_rust() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
    }

    #[test]
    fn test_language_from_extension_python() {
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("pyi"), Language::Python);
    }

    #[test]
    fn test_language_from_extension_ts() {
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("tsx"), Language::TypeScript);
    }

    #[test]
    fn test_language_from_extension_js() {
        assert_eq!(Language::from_extension("js"), Language::JavaScript);
        assert_eq!(Language::from_extension("mjs"), Language::JavaScript);
    }

    #[test]
    fn test_language_from_extension_others() {
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("java"), Language::Java);
        assert_eq!(Language::from_extension("scala"), Language::Scala);
        assert_eq!(Language::from_extension("vue"), Language::Vue);
        assert_eq!(Language::from_extension("sql"), Language::Sql);
    }

    #[test]
    fn test_language_from_extension_unknown() {
        assert_eq!(Language::from_extension("xyz"), Language::Other);
        assert_eq!(Language::from_extension(""), Language::Other);
    }

    #[test]
    fn test_filemark_from_str() {
        assert_eq!(FileMark::from_name("documentation"), Some(FileMark::Documentation));
        assert_eq!(FileMark::from_name("doc"), Some(FileMark::Documentation));
        assert_eq!(FileMark::from_name("test"), Some(FileMark::Test));
        assert_eq!(FileMark::from_name("ignore"), Some(FileMark::Ignore));
        assert_eq!(FileMark::from_name("config"), Some(FileMark::Config));
        assert_eq!(FileMark::from_name("generated"), Some(FileMark::Generated));
        assert_eq!(FileMark::from_name("custom"), Some(FileMark::Custom));
    }

    #[test]
    fn test_filemark_from_name_invalid() {
        assert_eq!(FileMark::from_name("unknown"), None);
        assert_eq!(FileMark::from_name(""), None);
    }

    #[test]
    fn test_tree_sitter_support() {
        assert!(Language::Rust.has_tree_sitter_support());
        assert!(Language::Python.has_tree_sitter_support());
        assert!(Language::TypeScript.has_tree_sitter_support());
        assert!(Language::JavaScript.has_tree_sitter_support());
        assert!(Language::Go.has_tree_sitter_support());
        assert!(!Language::Markdown.has_tree_sitter_support());
        assert!(!Language::Other.has_tree_sitter_support());
    }
}
