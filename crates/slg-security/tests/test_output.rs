//! Security invariant tests: output guard sanitization.
//! NEVER SKIP — runs on every CI build.

use slg_security::output_guard::OutputGuard;

#[test]
fn output_truncated_at_max_bytes() {
    let guard = OutputGuard::new();
    let large = "x".repeat(100_000);
    let result = guard.check_and_sanitize(&large, 50_000);

    assert!(result.len() <= 50_020); // max_bytes + "[OUTPUT TRUNCATED]"
}

#[test]
fn secrets_caught_in_final_output() {
    let guard = OutputGuard::new();
    let output = "Search result: found AKIAIOSFODNN7EXAMPLE1 in commit abc123";
    let result = guard.check_and_sanitize(output, 50_000);

    assert!(!result.contains("AKIAIOSFODNN7EXAMPLE1"));
    assert!(result.contains("[REDACTED"));
}

#[test]
fn normal_output_unchanged() {
    let guard = OutputGuard::new();
    let output = "fix: resolve login crash on timeout\n\nModified src/auth/session.ts";
    let result = guard.check_and_sanitize(output, 50_000);

    assert_eq!(result, output);
}

#[test]
fn empty_output_handled() {
    let guard = OutputGuard::new();
    let result = guard.check_and_sanitize("", 50_000);

    assert_eq!(result, "");
}

#[test]
fn zero_max_bytes_handled() {
    let guard = OutputGuard::new();
    let result = guard.check_and_sanitize("hello", 0);

    assert!(result.contains("[OUTPUT TRUNCATED]"));
}

#[test]
fn multiple_secrets_all_caught() {
    let guard = OutputGuard::new();
    let output = "key1=AKIAIOSFODNN7EXAMPLE1 token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
    let result = guard.check_and_sanitize(output, 50_000);

    assert!(!result.contains("AKIAIOSFODNN7EXAMPLE1"));
    assert!(!result.contains("ghp_"));
}
