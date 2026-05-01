use serde_json::{json, Value};
use crate::server::state::AppState;
use std::sync::atomic::Ordering;

pub fn get_stats(state: &AppState) -> Value {
    let uptime = chrono::Utc::now() - state.inner.start_time;
    
    let projects: Vec<Value> = state.inner.projects.iter().map(|entry| {
        let p = entry.value();
        let hits = p.file_cache.hits.load(Ordering::Relaxed);
        let misses = p.file_cache.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 { (hits as f64 / total as f64) * 100.0 } else { 0.0 };
        
        json!({
            "path": p.root.display().to_string(),
            "files": p.file_tree.len(),
            "symbols": p.symbol_table.len(),
            "cache": {
                "hits": hits,
                "misses": misses,
                "hit_rate": format!("{:.2}%", hit_rate),
                "total_bytes": *p.file_cache.total_bytes.lock(),
            }
        })
    }).collect();

    json!({
        "status": "ok",
        "uptime": format!("{}s", uptime.num_seconds()),
        "active_sessions": state.inner.sessions.len(),
        "total_projects": state.inner.projects.len(),
        "projects": projects,
    })
}
