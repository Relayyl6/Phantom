//! WASM Plugin Host — Phase 4 feature.
//!
//! Provides a secure WebAssembly runtime (via Extism) to run user-installed
//! plugins for integrations like Notion, Jira, Slack, and GitHub.
//!
//! Plugins can receive events (e.g. `RecordingStopped`, `SummaryGenerated`)
//! and perform actions like creating tickets or uploading transcripts.

use anyhow::Result;
use extism::{Plugin, Manifest, Wasm};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An event dispatched to plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEvent {
    pub event_type: String,
    pub session_id: String,
    pub payload: serde_json::Value,
}

pub struct PluginHost {
    plugins: HashMap<String, Plugin>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Load a WASM plugin from a byte slice.
    pub fn load_plugin(&mut self, name: &str, wasm_bytes: &[u8]) -> Result<()> {
        let wasm = Wasm::data(wasm_bytes.to_vec());
        let manifest = Manifest::new([wasm]);
        
        let plugin = Plugin::new(&manifest, [], true)?;
        self.plugins.insert(name.to_string(), plugin);
        tracing::info!("Loaded WASM plugin: {}", name);
        Ok(())
    }

    /// Dispatch an event to a specific plugin.
    pub fn dispatch_event(&mut self, plugin_name: &str, event: &PluginEvent) -> Result<String> {
        if let Some(plugin) = self.plugins.get_mut(plugin_name) {
            let event_json = serde_json::to_vec(event)?;
            let response = plugin.call::<&[u8], &str>("handle_event", &event_json)?;
            Ok(response.to_string())
        } else {
            anyhow::bail!("Plugin {} not found", plugin_name)
        }
    }

    /// Dispatch an event to all loaded plugins.
    pub fn broadcast_event(&mut self, event: &PluginEvent) {
        let keys: Vec<String> = self.plugins.keys().cloned().collect();
        for name in keys {
            match self.dispatch_event(&name, event) {
                Ok(resp) => tracing::debug!("Plugin {} handled event: {}", name, resp),
                Err(e) => tracing::warn!("Plugin {} failed to handle event: {}", name, e),
            }
        }
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
