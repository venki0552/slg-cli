use clap::Args;
use slg_core::errors::SlgError;

#[derive(Args)]
pub struct SyncArgs {
    /// Suppress non-result output
    #[arg(long)]
    pub silent: bool,
}

/// Manually trigger reindex (for CI use). Alias for `slg reindex`.
pub async fn run(args: SyncArgs) -> Result<(), SlgError> {
    super::reindex::run(super::reindex::ReindexArgs {
        delta_only: true,
        background: false,
        silent: args.silent,
    })
    .await
}
