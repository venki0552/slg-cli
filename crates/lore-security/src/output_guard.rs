use crate::redactor::SecretRedactor;
use std::io::Write;

/// Injection patterns to check in final output.
const OUTPUT_INJECTION_PATTERNS: &[&str] = &[
    "ignore previous",
    "ignore all previous",
    "system prompt",
    "<|system|>",
    "<|user|>",
    "<|assistant|>",
    "[inst]",
    "### instruction",
];

/// Final output check before sending to stdout or MCP.
/// SECURITY: Belt-and-suspenders check for injections and secrets that survived earlier layers.
pub struct OutputGuard {
    redactor: SecretRedactor,
}

impl OutputGuard {
    pub fn new() -> Self {
        Self {
            redactor: SecretRedactor::new(),
        }
    }

    /// Check and sanitize output before delivery.
    /// SECURITY: Final defense layer — catches anything that slipped through.
    /// Never panics, never returns empty if input was valid.
    pub fn check_and_sanitize(&self, output: &str, max_bytes: usize) -> String {
        let mut result = output.to_string();

        // Step 1: Truncate to max_bytes
        if result.len() > max_bytes {
            // Truncate at char boundary
            let mut end = max_bytes;
            while !result.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            result.truncate(end);
            result.push_str("\n[OUTPUT TRUNCATED]");
        }

        // Step 2: Check for injection patterns that survived
        let lower = result.to_lowercase();
        let mut found_injection = false;
        for pattern in OUTPUT_INJECTION_PATTERNS {
            if lower.contains(pattern) {
                found_injection = true;
                break;
            }
        }
        if found_injection {
            self.log_security_event("output_injection_detected", "Injection pattern found in final output");
        }

        // Step 3: Check for unescaped XML structure-breaking content
        if result.contains("</data>") && !result.contains("<![CDATA[") {
            self.log_security_event("xml_structure_risk", "Unescaped </data> found in output");
        }
        if result.contains("</lore_retrieval>") {
            let count = result.matches("</lore_retrieval>").count();
            if count > 1 {
                self.log_security_event("xml_structure_risk", "Multiple </lore_retrieval> tags found");
            }
        }

        // Step 4: Check for secrets that survived redaction
        let (redacted, secret_count) = self.redactor.redact(&result);
        if secret_count > 0 {
            self.log_security_event(
                "output_secret_leak",
                &format!("{} secret(s) caught in output guard", secret_count),
            );
            result = redacted;
        }

        result
    }

    /// Log a security event. Never fails — silently skips if log write fails.
    /// SECURITY: Never includes the actual suspicious content in the log.
    fn log_security_event(&self, event_type: &str, detail: &str) {
        let log_path = crate::paths::security_log_path();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let timestamp = chrono::Utc::now().to_rfc3339();
            // Intentionally ignore write errors — logging should never crash lore
            let _ = writeln!(file, "[{}] [{}] {}", timestamp, event_type, detail);
        }
    }
}

impl Default for OutputGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_output_passes_through() {
        let guard = OutputGuard::new();
        let input = "This is a normal search result about fixing login issues.";
        let result = guard.check_and_sanitize(input, 50_000);
        assert_eq!(result, input);
    }

    #[test]
    fn test_output_truncated_at_max_bytes() {
        let guard = OutputGuard::new();
        let input = "a".repeat(100_000);
        let result = guard.check_and_sanitize(&input, 50_000);
        assert!(result.len() <= 50_000 + 20); // +20 for "[OUTPUT TRUNCATED]"
    }

    #[test]
    fn test_secrets_caught_in_output() {
        let guard = OutputGuard::new();
        let input = "Result: API key is AKIAIOSFODNN7EXAMPLE1 found in commit";
        let result = guard.check_and_sanitize(input, 50_000);
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE1"));
        assert!(result.contains("[REDACTED"));
    }

    #[test]
    fn test_empty_input_handled() {
        let guard = OutputGuard::new();
        let result = guard.check_and_sanitize("", 50_000);
        assert_eq!(result, "");
    }

    #[test]
    fn test_max_bytes_zero_handled() {
        let guard = OutputGuard::new();
        let result = guard.check_and_sanitize("hello world", 0);
        assert!(result.contains("[OUTPUT TRUNCATED]"));
    }
}
