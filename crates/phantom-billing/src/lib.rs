//! phantom-billing — Stripe integration + local entitlement cache.

pub mod entitlement;
pub mod stripe_client;
pub mod paywall_ui;

pub use entitlement::{EntitlementStore, Tier};
pub use paywall_ui::PaywallState;
