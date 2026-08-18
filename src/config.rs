//! Configuration loading (profiles, backends, limits). Placeholder for v0.1.

use serde::Deserialize;

// Fields unused until config loading is wired in v0.1 (see ROADMAP.md).
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub profile: Option<String>,
}
