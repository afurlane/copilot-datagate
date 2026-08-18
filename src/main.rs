//! DataGate entrypoint. Real wiring (schema loader, backends, MCP tools)
//! will be added incrementally per the roadmap.

mod config;
mod policy;

use config::Config;
use policy::Policy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("DataGate starting (skeleton build, no backend wired yet)");

    let config_path =
        std::env::var("DATAGATE_CONFIG").unwrap_or_else(|_| "datagate.toml".to_string());
    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!(%err, path = %config_path, "no valid config found, starting with an empty (deny-all) policy");
            Config::default()
        }
    };
    let policy_table_count = config.policy.tables.len();
    let _policy = Policy::new(config.policy);
    tracing::info!(tables = policy_table_count, "policy engine ready");

    Ok(())
}
