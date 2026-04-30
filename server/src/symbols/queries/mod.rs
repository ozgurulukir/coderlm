pub mod go;
pub mod java;
pub mod python;
pub mod rust;
pub mod scala;
pub mod typescript;
pub mod vue;

use std::sync::OnceLock;

use crate::index::file_entry::Language;

static RUST_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();
static PYTHON_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();
static TYPESCRIPT_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();
static JAVASCRIPT_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();
static GO_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();
static JAVA_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();
static SCALA_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();
static VUE_CONFIG: OnceLock<LanguageConfig> = OnceLock::new();

/// Get the tree-sitter language and symbol query for a given language.
pub fn get_language_config(lang: Language) -> Option<&'static LanguageConfig> {
    match lang {
        Language::Rust => Some(RUST_CONFIG.get_or_init(rust::config)),
        Language::Python => Some(PYTHON_CONFIG.get_or_init(python::config)),
        Language::TypeScript => Some(TYPESCRIPT_CONFIG.get_or_init(typescript::config)),
        Language::JavaScript => Some(JAVASCRIPT_CONFIG.get_or_init(typescript::js_config)),
        Language::Go => Some(GO_CONFIG.get_or_init(go::config)),
        Language::Java => Some(JAVA_CONFIG.get_or_init(java::config)),
        Language::Scala => Some(SCALA_CONFIG.get_or_init(scala::config)),
        Language::Vue => Some(VUE_CONFIG.get_or_init(vue::config)),
        _ => None,
    }
}

#[allow(dead_code)]
pub struct LanguageConfig {
    pub language: tree_sitter::Language,
    pub symbols_query: &'static str,
    /// Tree-sitter query for call expressions. Captures `@callee` for the called name.
    pub callers_query: &'static str,
    /// Tree-sitter query for local variable bindings. Captures `@var.name`.
    pub variables_query: &'static str,
    pub test_patterns: Vec<TestPattern>,
}

#[allow(dead_code)]
pub enum TestPattern {
    /// Match functions whose name starts with a prefix (e.g., "test_" in Python)
    FunctionPrefix(&'static str),
    /// Match functions with a specific attribute/decorator (e.g., #[test] in Rust)
    Attribute(&'static str),
    /// Match call expressions (e.g., it(), test(), describe() in JS/TS)
    CallExpression(&'static str),
}
