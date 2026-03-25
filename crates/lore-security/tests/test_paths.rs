//! Security invariant tests: path traversal is always blocked.
//! NEVER SKIP — runs on every CI build.

use lore_security::paths::safe_index_path;

#[test]
fn path_traversal_branch_blocked() {
    let result = safe_index_path("abc123", "../../.ssh/authorized_keys").unwrap();
    let result_str = result.to_string_lossy().replace('\\', "/");

    assert!(result_str.contains("abc123"));
    assert!(result_str.contains(".lore/indices/"));
    assert!(!result_str.contains(".."));
}

#[test]
fn path_traversal_absolute_blocked() {
    let result = safe_index_path("abc123", "/etc/passwd").unwrap();
    let result_str = result.to_string_lossy().replace('\\', "/");

    assert!(result_str.contains("abc123"));
    // Slashes are stripped: "/etc/passwd" becomes "etcpasswd"
    // The path is safely under the index directory
    assert!(result_str.contains(".lore/indices/abc123/"));
    assert!(!result_str.contains("/etc/passwd"));
}

#[test]
fn unicode_path_traversal_blocked() {
    let result = safe_index_path("abc123", "..%2F..%2F.ssh").unwrap();
    let result_str = result.to_string_lossy().replace('\\', "/");

    assert!(result_str.contains("abc123"));
    assert!(!result_str.contains(".ssh"));
}

#[test]
fn empty_branch_name_handled() {
    let result = safe_index_path("abc123", "").unwrap();
    let result_str = result.to_string_lossy();

    assert!(result_str.contains("unknown-branch"));
}

#[test]
fn long_branch_name_truncated() {
    let long_name = "a".repeat(200);
    let result = safe_index_path("abc123", &long_name).unwrap();
    let result_str = result.to_string_lossy().replace('\\', "/");

    assert!(result_str.contains("abc123"));
    assert!(result_str.contains(".lore/indices/"));
    // Branch name in path should be truncated
    let filename = result.file_name().unwrap().to_string_lossy();
    assert!(filename.len() <= 68); // 64 chars + ".db"
}

#[test]
fn null_bytes_in_branch_blocked() {
    let result = safe_index_path("abc123", "main\0evil").unwrap();
    let result_str = result.to_string_lossy();

    assert!(!result_str.contains('\0'));
    assert!(result_str.contains("abc123"));
}
