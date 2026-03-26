use thiserror::Error;

/// All slg errors with user-friendly messages.
/// No Rust internals exposed to users.
#[derive(Error, Debug)]
pub enum SlgError {
    #[error("Not a git repository. Run slg in a git repo or run 'git init' first.")]
    NotAGitRepo,

    #[error("No index found for this repository. Run 'slg init' to get started.")]
    NoIndex,

    #[error("Index is being built. Please wait a moment and try again.")]
    IndexBuilding,

    #[error("Path traversal attempt blocked: {0}")]
    PathTraversal(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Embedding model not found. Run 'slg init' to download it.")]
    ModelNotFound,

    #[error("Model checksum verification failed. Delete ~/.slg/models/ and run 'slg init' again.")]
    ModelChecksumFailed,

    #[error("MCP rate limit exceeded. Please wait before retrying.")]
    RateLimitExceeded,

    #[error("Query too long (max 500 chars). Please shorten your query.")]
    QueryTooLong,

    #[error("Index schema version mismatch. Run 'slg sync --reindex' to upgrade.")]
    SchemaMismatch,

    #[error("Git error: {0}")]
    Git(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Query is empty. Please provide a search query.")]
    EmptyQuery,
}
