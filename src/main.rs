use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = rustfox::AppConfig::from_env();
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(rustfox::run(config))
}
