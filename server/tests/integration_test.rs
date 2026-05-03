use std::path::Path;
use std::sync::Arc;

#[test]
fn test_symbol_table_operations() {
    use coderlm_server::symbols::symbol::{Symbol, SymbolKind};
    use coderlm_server::symbols::SymbolTable;
    use coderlm_server::index::file_entry::Language;

    let table = SymbolTable::new();
    let sym = Symbol {
        name: "test_fn".to_string(),
        kind: SymbolKind::Function,
        file: "src/lib.rs".to_string(),
        byte_range: (0, 10),
        line_range: (1, 3),
        language: Language::Rust,
        signature: "fn test_fn()".to_string(),
        definition: None,
        parent: None,
    };
    table.insert(sym);
    assert_eq!(table.len(), 1);
}

#[test]
fn test_file_tree_operations() {
    use coderlm_server::index::file_entry::FileEntry;
    use coderlm_server::index::file_tree::FileTree;
    use chrono::Utc;

    let tree = FileTree::new();
    let entry = FileEntry::new("src/lib.rs".to_string(), 100, Utc::now());
    tree.insert(entry);
    assert_eq!(tree.len(), 1);

    let found = tree.get("src/lib.rs");
    assert!(found.is_some());

    let removed = tree.remove("src/lib.rs");
    assert!(removed.is_some());
    assert_eq!(tree.len(), 0);
}

#[test]
fn test_render_tree_with_structure() {
    use coderlm_server::index::file_entry::FileEntry;
    use coderlm_server::index::file_tree::FileTree;
    use chrono::Utc;

    let tree = FileTree::new();
    tree.insert(FileEntry::new("src/main.rs".to_string(), 50, Utc::now()));
    tree.insert(FileEntry::new("src/lib.rs".to_string(), 100, Utc::now()));
    tree.insert(FileEntry::new("README.md".to_string(), 30, Utc::now()));

    let rendered = tree.render_tree(0);
    assert!(rendered.contains("main.rs"));
    assert!(rendered.contains("lib.rs"));
    assert!(rendered.contains("README.md"));
    assert!(rendered.contains("src/"));
}

#[test]
fn test_language_breakdown() {
    use coderlm_server::index::file_entry::FileEntry;
    use coderlm_server::index::file_tree::FileTree;
    use chrono::Utc;

    let tree = FileTree::new();
    tree.insert(FileEntry::new("main.rs".to_string(), 50, Utc::now()));
    tree.insert(FileEntry::new("lib.rs".to_string(), 100, Utc::now()));
    tree.insert(FileEntry::new("app.py".to_string(), 30, Utc::now()));

    let breakdown = tree.language_breakdown();
    assert_eq!(breakdown.len(), 2);
    let rust_entry = breakdown.iter().find(|b| b.language == coderlm_server::index::file_entry::Language::Rust).unwrap();
    assert_eq!(rust_entry.count, 2);
    let py_entry = breakdown.iter().find(|b| b.language == coderlm_server::index::file_entry::Language::Python).unwrap();
    assert_eq!(py_entry.count, 1);
}
