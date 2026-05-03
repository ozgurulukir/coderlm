use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::Mutex;
use tracing::{debug, info};

use crate::index::file_tree::FileTree;
use crate::index::{walker, watcher};
use crate::server::errors::AppError;
use crate::server::session::Session;
use crate::symbols::{parser, SymbolTable};

/// Simple LRU-ish file content cache to avoid redundant disk I/O.
pub struct FileCache {
    cache: Mutex<HashMap<String, Arc<str>>>,
    pub total_bytes: Mutex<usize>,
    pub max_bytes: usize,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
}

impl FileCache {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            total_bytes: Mutex::new(0),
            max_bytes,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    pub fn get_or_read(&self, abs_path: &Path, rel_path: &str) -> Result<Arc<str>, AppError> {
        // Fast path: check without holding the lock longer than needed
        {
            let cache = self.cache.lock();
            if let Some(content) = cache.get(rel_path) {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(content.clone());
            }
        }

        // Slow path: read from disk
        let content_raw = std::fs::read_to_string(abs_path)
            .map_err(|e| AppError::NotFound(format!("Failed to read {}: {}", rel_path, e)))?;
        let content: Arc<str> = Arc::from(content_raw);
        let bytes = content.len();

        // Don't cache files larger than the entire cache
        if bytes > self.max_bytes {
            return Ok(content);
        }

        let mut cache = self.cache.lock();
        if cache.contains_key(rel_path) {
            return Ok(content);
        }

        self.misses.fetch_add(1, Ordering::Relaxed);

        let mut total = self.total_bytes.lock();

        // Simple eviction: clear entire cache if over capacity
        if *total + bytes > self.max_bytes {
            debug!("File cache over capacity ({} bytes), clearing", *total);
            cache.clear();
            *total = 0;
        }

        cache.insert(rel_path.to_string(), content.clone());
        *total += bytes;

        Ok(content)
    }

    pub fn invalidate(&self, rel_path: &str) {
        let mut cache = self.cache.lock();
        if let Some(removed) = cache.remove(rel_path) {
            let mut total = self.total_bytes.lock();
            *total = total.saturating_sub(removed.len());
            debug!("Invalidated {} in file cache", rel_path);
        }
    }
}

/// Cache for tree-sitter parse trees to avoid redundant parsing.
pub struct ParseCache {
    pub trees: Mutex<HashMap<String, (tree_sitter::Tree, usize)>>, // (tree, source_len)
    pub max_entries: usize,
}

impl ParseCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            trees: Mutex::new(HashMap::new()),
            max_entries,
        }
    }

    pub fn insert(&self, rel_path: String, tree: tree_sitter::Tree, len: usize) {
        let mut trees = self.trees.lock();
        if trees.len() >= self.max_entries {
            debug!("Parse cache full ({} entries), clearing", trees.len());
            trees.clear();
        }
        trees.insert(rel_path, (tree, len));
    }

    pub fn invalidate(&self, rel_path: &str) {
        let mut trees = self.trees.lock();
        trees.remove(rel_path);
        debug!("Invalidated {} in parse cache", rel_path);
    }
}

/// A single indexed project with its own file tree, symbol table, and watcher.
pub struct Project {
    pub root: PathBuf,
    pub file_tree: Arc<FileTree>,
    pub symbol_table: Arc<SymbolTable>,
    pub file_cache: Arc<FileCache>,
    pub parse_cache: Arc<ParseCache>,
    pub watcher: Option<watcher::WatcherHandle>,
    pub last_active: Mutex<DateTime<Utc>>,
    /// Signal set to `true` when initial symbol extraction completes.
    pub extraction_done: Arc<AtomicU64>,
}

/// Shared application state, wrapped in Arc for axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub projects: DashMap<PathBuf, Arc<Project>>,
    pub sessions: DashMap<String, Session>,
    pub max_projects: usize,
    pub max_file_size: u64,
    pub start_time: DateTime<Utc>,
}

impl AppState {
    pub fn new(max_projects: usize, max_file_size: u64) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                projects: DashMap::new(),
                sessions: DashMap::new(),
                max_projects,
                max_file_size,
                start_time: Utc::now(),
            }),
        }
    }

    /// Look up an existing project or index a new one. Evicts LRU if at capacity.
    pub fn get_or_create_project(&self, cwd: &Path) -> Result<Arc<Project>, AppError> {
        let canonical = cwd.canonicalize().map_err(|e| {
            AppError::BadRequest(format!("Path not accessible: {}", e))
        })?;

        if !canonical.is_dir() {
            return Err(AppError::BadRequest(format!(
                "'{}' is not a directory",
                canonical.display()
            )));
        }

        // Return existing project if found
        if let Some(project) = self.inner.projects.get(&canonical) {
            *project.last_active.lock() = Utc::now();
            return Ok(project.clone());
        }

        // Check capacity, evict if needed
        if self.inner.projects.len() >= self.inner.max_projects {
            self.evict_lru()?;
        }

        // Scan directory
        let file_tree = Arc::new(FileTree::new());
        let symbol_table = Arc::new(SymbolTable::new());
        let file_cache = Arc::new(FileCache::new(crate::config::DEFAULT_FILE_CACHE_BYTES));
        let parse_cache = Arc::new(ParseCache::new(crate::config::DEFAULT_PARSE_CACHE_ENTRIES));
        let max_file_size = self.inner.max_file_size;

        info!("Indexing new project: {}", canonical.display());
        let file_count =
            walker::scan_directory(&canonical, &file_tree, max_file_size)
                .map_err(|e| AppError::Internal(e.to_string()))?;
        info!("Indexed {} files for {}", file_count, canonical.display());

        // Start watcher
        let watcher_handle = watcher::start_watcher(
            &canonical,
            file_tree.clone(),
            symbol_table.clone(),
            file_cache.clone(),
            parse_cache.clone(),
            max_file_size,
        )
        .ok();

        let extraction_done = Arc::new(AtomicU64::new(0));
        let project = Arc::new(Project {
            root: canonical.clone(),
            file_tree: file_tree.clone(),
            symbol_table: symbol_table.clone(),
            file_cache,
            parse_cache,
            watcher: watcher_handle,
            last_active: Mutex::new(Utc::now()),
            extraction_done: extraction_done.clone(),
        });

        self.inner.projects.insert(canonical, project.clone());

        // Spawn symbol extraction in background
        let ft = file_tree;
        let st = symbol_table;
        let root = project.root.clone();
        tokio::spawn(async move {
            info!("Starting symbol extraction for {}...", root.display());
            match parser::extract_all_symbols(&root, &ft, &st).await {
                Ok(count) => info!("Extracted {} symbols for {}", count, root.display()),
                Err(e) => tracing::error!("Symbol extraction failed for {}: {}", root.display(), e),
            }
            extraction_done.fetch_add(1, Ordering::Release);
        });

        Ok(project)
    }

    /// Look up the project for a given session. Returns a descriptive error if
    /// the project has been evicted.
    pub fn get_project_for_session(&self, session_id: &str) -> Result<Arc<Project>, AppError> {
        let session = self
            .inner
            .sessions
            .get(session_id)
            .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

        let project_path = &session.project_path;

        let project = self
            .inner
            .projects
            .get(project_path)
            .ok_or_else(|| {
                AppError::Gone(format!(
                    "Project at '{}' was evicted due to capacity limits. \
                     Start a new session to re-index, or increase --max-projects.",
                    project_path.display()
                ))
            })?;

        Ok(project.clone())
    }

    /// Update the last-active timestamp on a project.
    pub fn touch_project(&self, project_path: &Path) {
        if let Some(project) = self.inner.projects.get(project_path) {
            *project.last_active.lock() = Utc::now();
        }
    }

    /// Evict the least recently used project. Removes all sessions pointing to it.
    fn evict_lru(&self) -> Result<(), AppError> {
        // Find the project with the oldest last_active
        let oldest = self
            .inner
            .projects
            .iter()
            .min_by_key(|entry| *entry.value().last_active.lock())
            .map(|entry| entry.key().clone());

        let path = oldest.ok_or_else(|| {
            AppError::Internal("No projects to evict".into())
        })?;

        info!("Evicting project: {}", path.display());

        // Remove the project (drops watcher)
        self.inner.projects.remove(&path);

        // Remove all sessions attached to this project
        self.inner.sessions.retain(|_, session| session.project_path != path);

        Ok(())
    }
}
