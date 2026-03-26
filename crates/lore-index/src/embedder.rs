use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use lore_core::errors::LoreError;
use lore_core::types::CommitDoc;
use tracing::debug;

const MODEL_NAME: EmbeddingModel = EmbeddingModel::AllMiniLML6V2;
const EMBEDDING_DIM: usize = 384;

/// Embedding engine using fastembed's all-MiniLM-L6-v2 model.
pub struct Embedder {
    model: TextEmbedding,
}

impl Embedder {
    /// Create a new embedder. Downloads the model on first use.
    pub fn new() -> Result<Self, LoreError> {
        let cache_dir = lore_security::paths::models_dir();
        debug!("Loading embedding model from {:?}", cache_dir);

        let options = InitOptions::new(MODEL_NAME)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(true);

        let model = TextEmbedding::try_new(options)
            .map_err(|e| LoreError::Embedding(format!("Failed to load embedding model: {}", e)))?;

        Ok(Self { model })
    }

    /// Embed a single commit document.
    /// Builds a search-optimized text representation of the commit.
    pub fn embed_commit(&self, doc: &CommitDoc) -> Result<Vec<f32>, LoreError> {
        let text = build_commit_text(doc);
        self.embed_text(&text)
    }

    /// Embed a raw query string.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>, LoreError> {
        let truncated = if query.len() > 2000 {
            &query[..2000]
        } else {
            query
        };
        self.embed_text(truncated)
    }

    /// Batch embed multiple commit documents.
    pub fn embed_batch(&self, docs: &[&CommitDoc]) -> Result<Vec<Vec<f32>>, LoreError> {
        let texts: Vec<String> = docs.iter().map(|d| build_commit_text(d)).collect();
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        self.model
            .embed(text_refs, None)
            .map_err(|e| LoreError::Embedding(format!("Batch embedding failed: {}", e)))
    }

    /// Embed a single text string.
    fn embed_text(&self, text: &str) -> Result<Vec<f32>, LoreError> {
        let results = self
            .model
            .embed(vec![text], None)
            .map_err(|e| LoreError::Embedding(format!("Embedding failed: {}", e)))?;

        results
            .into_iter()
            .next()
            .ok_or_else(|| LoreError::Embedding("No embedding result returned".to_string()))
    }

    /// Get the embedding dimension.
    pub fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

/// Build the text representation of a commit for embedding.
fn build_commit_text(doc: &CommitDoc) -> String {
    let intent = format!("{:?}", doc.intent);
    let files = if doc.files_changed.is_empty() {
        String::new()
    } else {
        format!(
            "\nFiles: {}",
            doc.files_changed
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let issues = if doc.linked_issues.is_empty() {
        String::new()
    } else {
        format!("\nIssues: #{}", doc.linked_issues.join(", #"))
    };
    let diff = if doc.diff_summary.is_empty() {
        String::new()
    } else {
        // Truncate diff summary to keep total under ~2000 chars
        let max_diff = 1500;
        let d = if doc.diff_summary.len() > max_diff {
            &doc.diff_summary[..max_diff]
        } else {
            &doc.diff_summary
        };
        format!("\nSummary: {}", d)
    };

    format!("{}: {}{}{}{}", intent, doc.message, files, issues, diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lore_core::types::CommitIntent;

    #[test]
    fn test_build_commit_text() {
        let doc = CommitDoc {
            hash: "abc".to_string(),
            short_hash: "abc".to_string(),
            message: "fix: resolve login crash".to_string(),
            body: None,
            diff_summary: "src/auth.rs: modified".to_string(),
            author: "Test".to_string(),
            timestamp: 0,
            files_changed: vec!["src/auth.rs".to_string()],
            insertions: 0,
            deletions: 0,
            linked_issues: vec!["123".to_string()],
            linked_prs: vec![],
            intent: CommitIntent::Fix,
            risk_score: 0.0,
            branch: "main".to_string(),
            injection_flagged: false,
            secrets_redacted: 0,
        };

        let text = build_commit_text(&doc);
        assert!(text.contains("Fix"));
        assert!(text.contains("fix: resolve login crash"));
        assert!(text.contains("src/auth.rs"));
        assert!(text.contains("#123"));
    }
}
