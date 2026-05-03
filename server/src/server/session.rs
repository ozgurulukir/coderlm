use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Maximum number of history entries to keep per session.
/// Prevents unbounded memory growth in long-running sessions.
const MAX_HISTORY: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub method: String,
    pub path: String,
    pub response_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub history: Vec<HistoryEntry>,
}

impl Session {
    pub fn new(id: String, project_path: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            id,
            project_path,
            created_at: now,
            last_active: now,
            history: Vec::new(),
        }
    }

    pub fn record(&mut self, method: &str, path: &str, response_preview: &str) {
        if self.history.len() >= MAX_HISTORY {
            self.history.remove(0);
        }
        self.last_active = Utc::now();
        self.history.push(HistoryEntry {
            timestamp: Utc::now(),
            method: method.to_string(),
            path: path.to_string(),
            response_preview: if response_preview.len() > 200 {
                format!("{}...", &response_preview[..200])
            } else {
                response_preview.to_string()
            },
        });
    }
}
