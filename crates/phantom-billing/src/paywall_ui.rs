//! Paywall UI data structures — soft gates, upgrade prompts.
//!
//! Provides the UI state and messages needed to show a soft paywall
//! when a user hits a Pro feature or daily AI limit.

use serde::{Deserialize, Serialize};

/// Represents the state of an upgrade prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaywallState {
    pub is_visible: bool,
    pub title: String,
    pub description: String,
    pub feature_name: String,
}

impl Default for PaywallState {
    fn default() -> Self {
        Self {
            is_visible: false,
            title: "Upgrade to Phantom Pro".to_string(),
            description: "Get unlimited AI summaries, smart chapters, and priority processing.".to_string(),
            feature_name: "Pro Features".to_string(),
        }
    }
}

impl PaywallState {
    /// Create a paywall state for a specific locked feature.
    pub fn for_feature(feature_name: &str, description: &str) -> Self {
        Self {
            is_visible: true,
            title: "Upgrade to Phantom Pro".to_string(),
            description: description.to_string(),
            feature_name: feature_name.to_string(),
        }
    }

    /// Create a paywall state when the daily Nano summary limit is hit.
    pub fn daily_limit_reached() -> Self {
        Self {
            is_visible: true,
            title: "Daily Limit Reached".to_string(),
            description: "You've used all 3 of your free daily AI summaries. Upgrade to Pro for unlimited on-device and cloud intelligence.".to_string(),
            feature_name: "Unlimited Summaries".to_string(),
        }
    }

    pub fn dismiss(&mut self) {
        self.is_visible = false;
    }
}
