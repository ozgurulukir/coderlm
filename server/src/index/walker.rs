use anyhow::Result;
use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

use crate::config;
use crate::index::file_entry::FileEntry;
use crate::index::file_tree::FileTree;

/// Scan the codebase directory using the `ignore` crate (respects .gitignore)
/// plus our built-in ignore patterns. Returns the number of files indexed.
pub fn scan_directory(root: &Path, file_tree: &Arc<FileTree>, max_file_size: u64) -> Result<usize> {
    let walker = WalkBuilder::new(root)
        .hidden(true) // skip dotfiles by default
        .git_ignore(true) // respect .gitignore
        .git_global(true)
        .git_exclude(true)
        .build();

    let mut count = 0;

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Skip directories
        if entry.file_type().is_none_or(|ft| ft.is_dir()) {
            continue;
        }

        let path = entry.path();

        // Get the relative path
        let rel_path = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        // Apply our additional ignore rules
        if should_skip(&rel_path) {
            continue;
        }

        // Check extension-based ignoring
        if config::should_ignore_extension(&rel_path) {
            continue;
        }

        // Get file metadata
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let size = metadata.len();

        // Skip files over size limit (they still won't appear in the tree)
        if size > max_file_size {
            continue;
        }

        let modified: DateTime<Utc> = metadata
            .modified()
            .map(DateTime::from)
            .unwrap_or_else(|_| Utc::now());

        let file_entry = FileEntry::new(rel_path, size, modified);
        file_tree.insert(file_entry);
        count += 1;
    }

    info!("Scanned {} files from {}", count, root.display());
    Ok(count)
}

/// Check if any path component matches our built-in ignore directories.
fn should_skip(rel_path: &str) -> bool {
    for component in rel_path.split('/') {
        if config::should_ignore_dir(component) {
            return true;
        }
    }
    false
}
