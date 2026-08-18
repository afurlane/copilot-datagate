//! DataGate entrypoint. Real wiring (config, policy engine, schema loader,
//! backends, MCP tools) will be added incrementally per the roadmap.

mod config;
mod policy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("DataGate starting (skeleton build, no backend wired yet)");

    let _config = config::Config::default();
    let _policy = policy::Policy::default();

    Ok(())
}
