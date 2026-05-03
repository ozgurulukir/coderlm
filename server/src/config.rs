/// Default ignore patterns applied on top of .gitignore rules.
/// These are directory names or file patterns that are almost never useful
/// for code intelligence.
pub const DEFAULT_IGNORE_DIRS: &[&str] = &[
    "node_modules",
    "vendor",
    "__pycache__",
    ".pycache",
    "target",
    "dist",
    "build",
    ".git",
    ".hg",
    ".svn",
    ".next",
    ".nuxt",
    ".output",
    ".cache",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "venv",
    ".venv",
    "env",
    ".env",
    "coverage",
    ".coverage",
    ".nyc_output",
    "htmlcov",
    ".terraform",
    ".serverless",
];

/// File extensions that are binary or otherwise useless for code reading.
pub const DEFAULT_IGNORE_EXTENSIONS: &[&str] = &[
    "min.js", "min.css", "pyc", "pyo", "class", "o", "so", "dylib", "dll", "exe", "a", "lib",
    "jar", "war", "ear", "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "png", "jpg", "jpeg",
    "gif", "bmp", "ico", "svg", "webp", "mp3", "mp4", "avi", "mov", "wmv", "flv", "woff",
    "woff2", "ttf", "eot", "otf", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "db",
    "sqlite", "sqlite3", "lock", "map",
];

/// Maximum file size (in bytes) to index by default. Files larger than this
/// are still listed in the tree but are not parsed for symbols.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1_000_000; // 1 MB

/// Maximum bytes for the in-memory file content cache (default: 50 MB).
pub const DEFAULT_FILE_CACHE_BYTES: usize = 50 * 1024 * 1024;

/// Maximum number of tree-sitter parse trees to cache.
pub const DEFAULT_PARSE_CACHE_ENTRIES: usize = 200;

pub fn should_ignore_dir(name: &str) -> bool {
    DEFAULT_IGNORE_DIRS.contains(&name)
}

pub fn should_ignore_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    DEFAULT_IGNORE_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(&format!(".{}", ext)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_ignore_dir_common() {
        assert!(should_ignore_dir("node_modules"));
        assert!(should_ignore_dir("target"));
        assert!(should_ignore_dir(".git"));
        assert!(should_ignore_dir("__pycache__"));
        assert!(should_ignore_dir("dist"));
    }

    #[test]
    fn test_should_ignore_dir_normal() {
        assert!(!should_ignore_dir("src"));
        assert!(!should_ignore_dir("lib"));
        assert!(!should_ignore_dir("tests"));
    }

    #[test]
    fn test_should_ignore_extension_binary() {
        assert!(should_ignore_extension("foo.min.js"));
        assert!(should_ignore_extension("bar.pyc"));
        assert!(should_ignore_extension("lib.o"));
        assert!(should_ignore_extension("archive.zip"));
        assert!(should_ignore_extension("image.png"));
    }

    #[test]
    fn test_should_ignore_extension_source() {
        assert!(!should_ignore_extension("main.rs"));
        assert!(!should_ignore_extension("lib.py"));
        assert!(!should_ignore_extension("index.js"));
        assert!(!should_ignore_extension("style.css"));
        assert!(!should_ignore_extension("page.html"));
    }

    #[test]
    fn test_should_ignore_extension_path_with_slashes() {
        assert!(should_ignore_extension("dist/bundle.min.js"));
        assert!(!should_ignore_extension("src/main.rs"));
    }
}
