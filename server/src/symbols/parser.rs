use anyhow::Result;
use std::path::Path;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tree_sitter::StreamingIterator;
use tracing::{debug, warn};

use crate::index::file_entry::Language;
use crate::index::file_tree::FileTree;
use crate::server::state::ParseCache;
use crate::symbols::queries;
use crate::symbols::symbol::{Symbol, SymbolKind};
use crate::symbols::SymbolTable;

/// Get or parse a tree-sitter tree for a file, using the provided cache.
pub fn get_parse_tree(
    rel_path: &str,
    source: &str,
    language: Language,
    parse_cache: &ParseCache,
) -> Result<tree_sitter::Tree> {
    {
        let trees = parse_cache.trees.lock();
        if let Some((tree, len)) = trees.get(rel_path) {
            // Simple validation: check if length matches. 
            // In a real system, we'd use a hash of the source.
            if *len == source.len() {
                return Ok(tree.clone());
            }
        }
    }

    let config = queries::get_language_config(language)
        .ok_or_else(|| anyhow::anyhow!("No tree-sitter config for {:?}", language))?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&config.language)?;
    let tree = parser.parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse {}", rel_path))?;

    parse_cache.insert(rel_path.to_string(), tree.clone(), source.len());
    Ok(tree)
}

/// Extract the content of the first `<script>` block from a Vue SFC.
/// Falls back to the entire file content if no script block is found.
fn extract_vue_script_content(source: &str) -> String {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_vue_updated::language())
        .expect("Failed to load tree-sitter-vue grammar");

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return source.to_string(),
    };

    // Walk the AST to find the first script_element and its raw_text child
    fn find_script_raw<'a>(node: tree_sitter::Node<'a>, source: &str) -> Option<String> {
        if node.kind() == "raw_text" {
            // Check if parent is a script_element
            if let Some(parent) = node.parent() {
                if parent.kind() == "script_element" {
                    return Some(node.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if let Some(content) = find_script_raw(child, source) {
                    return Some(content);
                }
            }
        }
        None
    }

    find_script_raw(tree.root_node(), source).unwrap_or_default()
}

/// Extract symbols and identifier references from a single file.
pub fn extract_symbols_from_file(
    root: &Path,
    rel_path: &str,
    language: Language,
) -> Result<(Vec<Symbol>, HashSet<String>)> {
    let abs_path = root.join(rel_path);
    let raw_source = std::fs::read_to_string(&abs_path)?;

    // For Vue files, extract the <script> content and parse it as TypeScript.
    let (_source, effective_language) = if language == Language::Vue {
        let script_content = extract_vue_script_content(&raw_source);
        if script_content.is_empty() {
            debug!("No <script> block found in {}", rel_path);
            return Ok((Vec::new(), HashSet::new()));
        }
        (script_content, Language::TypeScript)
    } else {
        (raw_source, language)
    };

    let config = match queries::get_language_config(effective_language) {
        Some(c) => c,
        None => return Ok((Vec::new(), HashSet::new())),
    };

    let abs_path = root.join(rel_path);
    let source = std::fs::read_to_string(&abs_path)?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&config.language)?;

    let tree = match parser.parse(&source, None) {
        Some(t) => t,
        None => {
            warn!("Failed to parse {}", rel_path);
            return Ok((Vec::new(), HashSet::new()));
        }
    };

    // 1. Extract Symbols
    let mut symbols = Vec::new();
    {
        let query = tree_sitter::Query::new(&config.language, config.symbols_query)?;
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let capture_names: Vec<String> = query.capture_names().iter().map(|s| s.to_string()).collect();
        let mut current_impl_type: Option<String> = None;

        while let Some(m) = matches.next() {
            let mut name: Option<String> = None;
            let mut kind: Option<SymbolKind> = None;
            let mut def_node: Option<tree_sitter::Node> = None;
            let mut parent: Option<String> = None;

            for cap in m.captures {
                let cap_name = &capture_names[cap.index as usize];
                let text = cap.node.utf8_text(source.as_bytes()).unwrap_or("");

                match cap_name.as_str() {
                    "function.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Function);
                    }
                    "function.def" => {
                        def_node = Some(cap.node);
                    }
                    "method.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Method);
                        parent = current_impl_type.clone();
                    }
                    "method.def" => {
                        def_node = Some(cap.node);
                    }
                    "impl.type" => {
                        current_impl_type = Some(text.to_string());
                    }
                    "struct.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Struct);
                    }
                    "struct.def" => {
                        def_node = Some(cap.node);
                    }
                    "enum.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Enum);
                    }
                    "enum.def" => {
                        def_node = Some(cap.node);
                    }
                    "trait.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Trait);
                    }
                    "trait.def" => {
                        def_node = Some(cap.node);
                    }
                    "class.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Class);
                    }
                    "class.def" => {
                        def_node = Some(cap.node);
                    }
                    "interface.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Interface);
                    }
                    "interface.def" => {
                        def_node = Some(cap.node);
                    }
                    "type.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Type);
                    }
                    "type.def" => {
                        def_node = Some(cap.node);
                    }
                    "const.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Constant);
                    }
                    "const.def" => {
                        def_node = Some(cap.node);
                    }
                    "static.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Constant);
                    }
                    "static.def" => {
                        def_node = Some(cap.node);
                    }
                    "mod.name" => {
                        name = Some(text.to_string());
                        kind = Some(SymbolKind::Module);
                    }
                    "mod.def" => {
                        def_node = Some(cap.node);
                    }
                    _ => {}
                }
            }

            if let (Some(name), Some(kind), Some(node)) = (name, kind, def_node) {
                let start = node.start_position();
                let end = node.end_position();
                let byte_range = (node.start_byte(), node.end_byte());
                let line_range = (start.row + 1, end.row + 1); // 1-indexed

                // Extract signature (first line of the definition)
                let node_text = node.utf8_text(source.as_bytes()).unwrap_or("");
                let signature = node_text.lines().next().unwrap_or("").to_string();

                symbols.push(Symbol {
                    name,
                    kind,
                    file: rel_path.to_string(),
                    byte_range,
                    line_range,
                    language: if language == Language::Vue { Language::Vue } else { effective_language },
                    signature,
                    definition: None,
                    parent,
                });
            }
        }
    }

    // 2. Extract Identifier References (for Inverted Index)
    let mut refs = HashSet::new();
    {
        let query = tree_sitter::Query::new(&config.language, config.callers_query)?;
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let capture_names: Vec<String> = query.capture_names().iter().map(|s| s.to_string()).collect();
        let callee_idx = capture_names.iter().position(|n| n == "callee");

        while let Some(m) = matches.next() {
            for cap in m.captures {
                if Some(cap.index as usize) == callee_idx {
                    let text = cap.node.utf8_text(source.as_bytes()).unwrap_or("");
                    if !text.is_empty() {
                        refs.insert(text.to_string());
                    }
                }
            }
        }
    }

    debug!("Extracted {} symbols and {} refs from {}", symbols.len(), refs.len(), rel_path);
    Ok((symbols, refs))
}

/// Extract symbols from all files in the tree. Runs on blocking threads
/// with bounded concurrency using rayon.
pub async fn extract_all_symbols(
    root: &Path,
    file_tree: &Arc<FileTree>,
    symbol_table: &Arc<SymbolTable>,
) -> Result<usize> {
    let root = root.to_path_buf();
    let file_tree = file_tree.clone();
    let symbol_table = symbol_table.clone();

    let count = tokio::task::spawn_blocking(move || -> Result<usize> {
        use rayon::prelude::*;
        let total = AtomicUsize::new(0);

        let paths: Vec<(String, Language)> = file_tree
            .files
            .iter()
            .filter(|e| e.value().language.has_tree_sitter_support())
            .map(|e| (e.key().clone(), e.value().language))
            .collect();

        paths.par_iter().for_each(|(rel_path, language)| {
            match extract_symbols_from_file(&root, rel_path, *language) {
                Ok((symbols, refs)) => {
                    let count = symbols.len();
                    for sym in symbols {
                        symbol_table.insert(sym);
                    }
                    for r in refs {
                        symbol_table.insert_id_ref(r, rel_path.clone());
                    }
                    // Mark file as having symbols extracted
                    if let Some(mut entry) = file_tree.files.get_mut(rel_path) {
                        entry.symbols_extracted = true;
                    }
                    total.fetch_add(count, Ordering::Relaxed);
                }
                Err(e) => {
                    debug!("Failed to extract symbols from {}: {}", rel_path, e);
                }
            }
        });

        Ok(total.load(Ordering::Relaxed))
    })
    .await??;

    Ok(count)
}
