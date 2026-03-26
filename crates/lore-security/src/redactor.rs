use regex::Regex;
use tracing::debug;

/// Secret patterns with their replacement labels.
/// SECURITY: Order matters — more specific patterns first to avoid partial matches.
const SECRET_PATTERNS: &[(&str, &str)] = &[
    // AWS access key (very specific prefix)
    (r"AKIA[A-Z0-9]{16}", "[REDACTED-AWS-ACCESS]"),
    // GitHub personal access token
    (r"ghp_[a-zA-Z0-9]{36}", "[REDACTED-GH-TOKEN]"),
    // GitHub fine-grained PAT
    (r"github_pat_[a-zA-Z0-9_]{82}", "[REDACTED-GH-PAT]"),
    // Anthropic API key
    (r"sk-ant-[a-zA-Z0-9\-]{32,}", "[REDACTED-ANTHROPIC]"),
    // OpenAI API key (must come after Anthropic to avoid overlap)
    (r"sk-[a-zA-Z0-9]{32,}", "[REDACTED-OPENAI]"),
    // Google API key
    (r"AIza[0-9A-Za-z\-_]{35}", "[REDACTED-GOOGLE]"),
    // Stripe live key
    (r"sk_live_[0-9a-zA-Z]{24,}", "[REDACTED-STRIPE-LIVE]"),
    // Stripe test key
    (r"sk_test_[0-9a-zA-Z]{24,}", "[REDACTED-STRIPE-TEST]"),
    // Twilio account SID
    (r"AC[a-z0-9]{32}", "[REDACTED-TWILIO]"),
    // Private key block
    (
        r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
        "[REDACTED-PRIVATE-KEY]",
    ),
    // JWT token
    (
        r"eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}",
        "[REDACTED-JWT]",
    ),
    // Database URL with credentials
    (
        r"(?:postgres|mysql|mongodb|redis)://[^\s:]+:[^\s@]+@",
        "[REDACTED-DB-URL]://",
    ),
    // Generic credential patterns (case-insensitive)
    (
        r"(?i)(?:api[_-]?key|secret|password|passwd|token|auth|credential)\s*[:=]\s*['\x22]?([a-zA-Z0-9_\-\.+/=]{8,})",
        "[REDACTED-GENERIC]",
    ),
];

/// Detects and redacts secrets from text before storage.
/// SECURITY: Secrets never reach the index. Applied to diff_summary before storage.
pub struct SecretRedactor {
    patterns: Vec<(Regex, &'static str)>,
}

impl SecretRedactor {
    pub fn new() -> Self {
        let patterns = SECRET_PATTERNS
            .iter()
            .filter_map(|(pattern, replacement)| {
                Regex::new(pattern).ok().map(|re| (re, *replacement))
            })
            .collect();

        Self { patterns }
    }

    /// Redact secrets from text. Returns (redacted_text, count_of_redactions).
    /// SECURITY: Never logs what was redacted — only counts.
    pub fn redact(&self, text: &str) -> (String, u32) {
        let mut result = text.to_string();
        let mut total_count: u32 = 0;

        for (regex, replacement) in &self.patterns {
            let matches: Vec<_> = regex.find_iter(&result).collect();
            let count = matches.len() as u32;
            if count > 0 {
                total_count += count;
                result = regex.replace_all(&result, *replacement).to_string();
                debug!("Redacted {} instances of secret pattern", count);
            }
        }

        (result, total_count)
    }
}

impl Default for SecretRedactor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_key_redacted() {
        let redactor = SecretRedactor::new();
        let text = "AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE1";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE1"));
        assert!(result.contains("[REDACTED-AWS-ACCESS]"));
        assert!(count >= 1);
    }

    #[test]
    fn test_github_token_redacted() {
        let redactor = SecretRedactor::new();
        let text = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("ghp_"));
        assert!(result.contains("[REDACTED-GH-TOKEN]"));
        assert!(count >= 1);
    }

    #[test]
    fn test_anthropic_key_redacted() {
        let redactor = SecretRedactor::new();
        let text = "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("sk-ant-"));
        assert!(result.contains("[REDACTED-ANTHROPIC]"));
        assert!(count >= 1);
    }

    #[test]
    fn test_openai_key_redacted() {
        let redactor = SecretRedactor::new();
        let text = "OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz1234567890ab";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("sk-abcdefghij"));
        assert!(count >= 1);
    }

    #[test]
    fn test_private_key_redacted() {
        let redactor = SecretRedactor::new();
        let text =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("BEGIN RSA PRIVATE KEY"));
        assert!(result.contains("[REDACTED-PRIVATE-KEY]"));
        assert!(count >= 1);
    }

    #[test]
    fn test_jwt_redacted() {
        let redactor = SecretRedactor::new();
        let text = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("eyJhbGciOiJIUzI1NiI"));
        assert!(result.contains("[REDACTED-JWT]"));
        assert!(count >= 1);
    }

    #[test]
    fn test_generic_password_redacted() {
        let redactor = SecretRedactor::new();
        let text = "password=SuperSecretPassword123";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("SuperSecretPassword123"));
        assert!(count >= 1);
    }

    #[test]
    fn test_db_url_redacted() {
        let redactor = SecretRedactor::new();
        let text = "DATABASE_URL=postgres://admin:secret@localhost:5432/mydb";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("admin:secret@"));
        assert!(count >= 1);
    }

    #[test]
    fn test_multiple_secrets_counted() {
        let redactor = SecretRedactor::new();
        let text = "key1=AKIAIOSFODNN7EXAMPLE1 key2=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let (_, count) = redactor.redact(text);
        assert!(count >= 2);
    }

    #[test]
    fn test_normal_text_not_redacted() {
        let redactor = SecretRedactor::new();
        let text = "This is a normal commit message about fixing the login page";
        let (result, count) = redactor.redact(text);
        assert_eq!(result, text);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_stripe_live_key_redacted() {
        let redactor = SecretRedactor::new();
        let text = "stripe_key = sk_live_abcdefghijklmnopqrstuvwx";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("sk_live_"));
        assert!(result.contains("[REDACTED-STRIPE-LIVE]"));
        assert!(count >= 1);
    }

    #[test]
    fn test_google_api_key_redacted() {
        let redactor = SecretRedactor::new();
        let text = "GOOGLE_API_KEY=AIzaSyA1234567890abcdefghijklmnopqrstuv";
        let (result, count) = redactor.redact(text);
        assert!(!result.contains("AIzaSy"));
        assert!(result.contains("[REDACTED-GOOGLE]"));
        assert!(count >= 1);
    }
}
