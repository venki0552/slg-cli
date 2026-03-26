use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static INDEXING_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Check if an index exists at the given path.
pub fn index_exists(index_path: &Path) -> bool {
    index_path.exists()
}

/// Check if indexing is currently in progress.
pub fn is_indexing() -> bool {
    INDEXING_IN_PROGRESS.load(Ordering::Relaxed)
}

/// Set the indexing-in-progress flag.
pub fn set_indexing(val: bool) {
    INDEXING_IN_PROGRESS.store(val, Ordering::Relaxed);
}

/// Generate the "initializing" response for when index doesn't exist yet.
pub fn initializing_response(tool_name: &str) -> Value {
    json!({
        "status": "initializing",
        "message": "slg is indexing this repository for the first time. Estimated time: ~15 seconds. Run your query again shortly.",
        "eta_seconds": 15,
        "tool": tool_name
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initializing_response() {
        let resp = initializing_response("slg_why");
        assert_eq!(resp["status"], "initializing");
        assert_eq!(resp["tool"], "slg_why");
        assert_eq!(resp["eta_seconds"], 15);
    }

    #[test]
    fn test_indexing_flag() {
        set_indexing(false);
        assert!(!is_indexing());
        set_indexing(true);
        assert!(is_indexing());
        set_indexing(false);
    }
}
