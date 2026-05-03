pub mod parser;
pub mod queries;
pub mod symbol;

use dashmap::DashMap;
use std::collections::HashSet;

use symbol::Symbol;

/// Thread-safe symbol table with secondary indices for fast lookup.
pub struct SymbolTable {
    /// Primary store: keyed by "file::name"
    pub symbols: DashMap<String, Symbol>,
    /// Secondary index: symbol name -> set of primary keys
    pub by_name: DashMap<String, HashSet<String>>,
    /// Secondary index: file path -> set of primary keys
    pub by_file: DashMap<String, HashSet<String>>,
    /// Inverted index: identifier name -> set of file paths containing it.
    /// Used to quickly narrow down files for caller/test discovery.
    pub id_refs: DashMap<String, HashSet<String>>,
    /// Reverse mapping: file path -> set of identifiers in that file.
    /// Used for O(1) cleanup of id_refs during file removal.
    pub file_to_ids: DashMap<String, HashSet<String>>,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: DashMap::new(),
            by_name: DashMap::new(),
            by_file: DashMap::new(),
            id_refs: DashMap::new(),
            file_to_ids: DashMap::new(),
        }
    }

    pub fn make_key(file: &str, name: &str) -> String {
        format!("{}::{}", file, name)
    }

    pub fn insert(&self, symbol: Symbol) {
        let key = Self::make_key(&symbol.file, &symbol.name);

        // Update secondary indices
        self.by_name
            .entry(symbol.name.clone())
            .or_default()
            .insert(key.clone());
        self.by_file
            .entry(symbol.file.clone())
            .or_default()
            .insert(key.clone());

        self.symbols.insert(key, symbol);
    }

    pub fn insert_id_ref(&self, id: String, file: String) {
        self.id_refs
            .entry(id.clone())
            .or_default()
            .insert(file.clone());
        self.file_to_ids
            .entry(file)
            .or_default()
            .insert(id);
    }

    pub fn remove_file(&self, file: &str) {
        if let Some((_, keys)) = self.by_file.remove(file) {
            for key in &keys {
                if let Some((_, sym)) = self.symbols.remove(key)
                    && let Some(mut name_set) = self.by_name.get_mut(&sym.name) {
                        name_set.remove(key);
                        if name_set.is_empty() {
                            drop(name_set);
                            self.by_name.remove(&sym.name);
                        }
                    }
            }
        }

        // Optimized cleanup of id_refs using file_to_ids
        if let Some((_, ids)) = self.file_to_ids.remove(file) {
            for id in ids {
                if let Some(mut files) = self.id_refs.get_mut(&id) {
                    files.remove(file);
                    if files.is_empty() {
                        drop(files);
                        self.id_refs.remove(&id);
                    }
                }
            }
        }
    }

    pub fn get(&self, file: &str, name: &str) -> Option<Symbol> {
        let key = Self::make_key(file, name);
        self.symbols.get(&key).map(|r| r.value().clone())
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Symbol> {
        use fuzzy_matcher::clangd::ClangdMatcher;
        use fuzzy_matcher::FuzzyMatcher;
        use rayon::prelude::*;

        let matcher = ClangdMatcher::default();

        let mut ranked_results: Vec<(i64, Symbol)> = self.symbols
            .par_iter()
            .filter_map(|entry| {
                matcher.fuzzy_match(&entry.value().name, query)
                    .map(|score| (score, entry.value().clone()))
            })
            .collect();

        // Sort by score descending
        ranked_results.sort_by_key(|b| std::cmp::Reverse(b.0));

        ranked_results.into_iter()
            .take(limit)
            .map(|(_, sym)| sym)
            .collect()
    }

    pub fn list_by_file(&self, file: &str) -> Vec<Symbol> {
        if let Some(keys) = self.by_file.get(file) {
            keys.iter()
                .filter_map(|key| self.symbols.get(key).map(|r| r.value().clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn all_symbols(&self) -> Vec<Symbol> {
        self.symbols.iter().map(|r| r.value().clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::file_entry::Language;
    use crate::symbols::symbol::{Symbol, SymbolKind};

    fn make_test_symbol(name: &str, file: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            file: file.to_string(),
            byte_range: (0, 1),
            line_range: (1, 1),
            language: Language::Rust,
            signature: format!("fn {}()", name),
            definition: None,
            parent: None,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let table = SymbolTable::new();
        table.insert(make_test_symbol("foo", "lib.rs"));
        let sym = table.get("lib.rs", "foo");
        assert!(sym.is_some());
        assert_eq!(sym.unwrap().name, "foo");
    }

    #[test]
    fn test_get_nonexistent() {
        let table = SymbolTable::new();
        assert!(table.get("lib.rs", "nonexistent").is_none());
    }

    #[test]
    fn test_insert_twice_overwrites() {
        let table = SymbolTable::new();
        table.insert(make_test_symbol("foo", "lib.rs"));
        let mut sym2 = make_test_symbol("foo", "lib.rs");
        sym2.signature = "fn foo(x: i32)".to_string();
        table.insert(sym2);
        let sym = table.get("lib.rs", "foo").unwrap();
        assert_eq!(sym.signature, "fn foo(x: i32)");
    }

    #[test]
    fn test_list_by_file() {
        let table = SymbolTable::new();
        table.insert(make_test_symbol("foo", "lib.rs"));
        table.insert(make_test_symbol("bar", "lib.rs"));
        table.insert(make_test_symbol("baz", "main.rs"));

        let lib_syms = table.list_by_file("lib.rs");
        assert_eq!(lib_syms.len(), 2);

        let main_syms = table.list_by_file("main.rs");
        assert_eq!(main_syms.len(), 1);
    }

    #[test]
    fn test_all_symbols() {
        let table = SymbolTable::new();
        table.insert(make_test_symbol("a", "a.rs"));
        table.insert(make_test_symbol("b", "b.rs"));
        assert_eq!(table.all_symbols().len(), 2);
    }

    #[test]
    fn test_len() {
        let table = SymbolTable::new();
        assert_eq!(table.len(), 0);
        table.insert(make_test_symbol("foo", "lib.rs"));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_remove_file() {
        let table = SymbolTable::new();
        table.insert(make_test_symbol("foo", "lib.rs"));
        table.insert(make_test_symbol("bar", "lib.rs"));
        assert_eq!(table.len(), 2);

        table.remove_file("lib.rs");
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn test_search() {
        let table = SymbolTable::new();
        table.insert(make_test_symbol("handle_request", "server.rs"));
        table.insert(make_test_symbol("handle_response", "server.rs"));
        table.insert(make_test_symbol("connect", "net.rs"));

        let results = table.search("handle", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_limit() {
        let table = SymbolTable::new();
        table.insert(make_test_symbol("foo1", "lib.rs"));
        table.insert(make_test_symbol("foo2", "lib.rs"));
        table.insert(make_test_symbol("foo3", "lib.rs"));

        let results = table.search("foo", 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_id_refs() {
        let table = SymbolTable::new();
        table.insert_id_ref("handle_request".to_string(), "server.rs".to_string());
        table.insert_id_ref("handle_request".to_string(), "client.rs".to_string());

        let refs = table.id_refs.get("handle_request").unwrap();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains("server.rs"));
        assert!(refs.contains("client.rs"));
    }
}
