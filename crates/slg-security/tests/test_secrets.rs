//! Security invariant tests: secrets are never stored.
//! NEVER SKIP — runs on every CI build.

use slg_security::redactor::SecretRedactor;

#[test]
fn aws_key_never_stored() {
    let redactor = SecretRedactor::new();
    let text = "AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE1 found in config";
    let (result, count) = redactor.redact(text);

    assert!(!result.contains("AKIAIOSFODNN7EXAMPLE1"));
    assert!(result.contains("[REDACTED-AWS-ACCESS]"));
    assert!(count >= 1);
}

#[test]
fn github_token_never_stored() {
    let redactor = SecretRedactor::new();
    let text = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
    let (result, count) = redactor.redact(text);

    assert!(!result.contains("ghp_"));
    assert!(result.contains("[REDACTED-GH-TOKEN]"));
    assert!(count >= 1);
}

#[test]
fn anthropic_key_never_stored() {
    let redactor = SecretRedactor::new();
    let text = "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890";
    let (result, count) = redactor.redact(text);

    assert!(!result.contains("sk-ant-"));
    assert!(result.contains("[REDACTED-ANTHROPIC]"));
    assert!(count >= 1);
}

#[test]
fn private_key_never_stored() {
    let redactor = SecretRedactor::new();
    let text =
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
    let (result, count) = redactor.redact(text);

    assert!(!result.contains("BEGIN RSA PRIVATE KEY"));
    assert!(result.contains("[REDACTED-PRIVATE-KEY]"));
    assert!(count >= 1);
}

#[test]
fn secrets_redacted_count_is_accurate() {
    let redactor = SecretRedactor::new();
    let text = "key1=AKIAIOSFODNN7EXAMPLE1 key2=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij key3=sk-abcdefghijklmnopqrstuvwxyz1234567890ab";
    let (result, count) = redactor.redact(text);

    assert!(
        count >= 3,
        "Expected at least 3 secrets redacted, got {}",
        count
    );
    assert!(!result.contains("AKIAIOSFODNN7EXAMPLE1"));
    assert!(!result.contains("ghp_"));
}

#[test]
fn jwt_never_stored() {
    let redactor = SecretRedactor::new();
    let text = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let (result, count) = redactor.redact(text);

    assert!(!result.contains("eyJhbGciOiJIUzI1NiI"));
    assert!(count >= 1);
}

#[test]
fn database_url_never_stored() {
    let redactor = SecretRedactor::new();
    let text = "DATABASE_URL=postgres://admin:secret@localhost:5432/mydb";
    let (result, count) = redactor.redact(text);

    assert!(!result.contains("admin:secret@"));
    assert!(count >= 1);
}

#[test]
fn normal_text_not_redacted() {
    let redactor = SecretRedactor::new();
    let text = "This is a normal commit message about fixing the login page and adding retry logic";
    let (result, count) = redactor.redact(text);

    assert_eq!(result, text);
    assert_eq!(count, 0);
}
