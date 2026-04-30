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
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: DashMap::new(),
            by_name: DashMap::new(),
            by_file: DashMap::new(),
            id_refs: DashMap::new(),
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
            .or_insert_with(HashSet::new)
            .insert(key.clone());
        self.by_file
            .entry(symbol.file.clone())
            .or_insert_with(HashSet::new)
            .insert(key.clone());

        self.symbols.insert(key, symbol);
    }

    pub fn insert_id_ref(&self, id: String, file: String) {
        self.id_refs
            .entry(id)
            .or_insert_with(HashSet::new)
            .insert(file);
    }

    pub fn remove_file(&self, file: &str) {
        if let Some((_, keys)) = self.by_file.remove(file) {
            for key in &keys {
                if let Some((_, sym)) = self.symbols.remove(key) {
                    if let Some(mut name_set) = self.by_name.get_mut(&sym.name) {
                        name_set.remove(key);
                        if name_set.is_empty() {
                            drop(name_set);
                            self.by_name.remove(&sym.name);
                        }
                    }
                }
            }
        }
        
        // Remove file from id_refs (expensive but necessary for correctness on file deletion)
        // We iterate over all entries because id_refs is keyed by identifier, not file.
        // In a real-world high-perf system, we'd keep a reverse by_file_ids index too.
        // For now, let's keep it simple.
        self.id_refs.retain(|_, files| {
            files.remove(file);
            !files.is_empty()
        });
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
        ranked_results.sort_by(|a, b| b.0.cmp(&a.0));

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
}
