use clap::Args;
use lore_core::errors::LoreError;

#[derive(Args)]
pub struct SyncArgs {
    /// Suppress non-result output
    #[arg(long)]
    pub silent: bool,
}

/// Manually trigger reindex (for CI use). Alias for `lore reindex`.
pub async fn run(args: SyncArgs) -> Result<(), LoreError> {
    super::reindex::run(super::reindex::ReindexArgs {
        delta_only: true,
        background: false,
        silent: args.silent,
    })
    .await
}
