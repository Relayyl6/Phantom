//! Stripe client — Checkout session creation and webhook handling.
//!
//! Full Stripe integration is Phase 2. This module defines the data types
//! and the HTTP interface so the rest of the codebase can compile and
//! reference them without the Stripe SDK.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const STRIPE_API: &str = "https://api.stripe.com/v1";

/// Stripe price IDs — replace with your real Stripe Dashboard IDs.
pub mod price_ids {
    pub const PRO_MONTHLY:  &str = "price_PHANTOM_PRO_MONTHLY";
    pub const TEAM_MONTHLY: &str = "price_PHANTOM_TEAM_MONTHLY";
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckoutSession {
    pub id: String,
    pub url: String,
}

pub struct StripeClient {
    secret_key: String,
    http: Client,
}

impl StripeClient {
    pub fn new(secret_key: String) -> Self {
        Self {
            secret_key,
            http: Client::new(),
        }
    }

    /// Create a Stripe Checkout session and return the redirect URL.
    /// `price_id` should be one of the constants in [`price_ids`].
    /// `customer_email` pre-fills the checkout form.
    pub async fn create_checkout_session(
        &self,
        price_id: &str,
        customer_email: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession> {
        let url = format!("{STRIPE_API}/checkout/sessions");
        
        let form = [
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("customer_email", customer_email),
            ("mode", "subscription"),
            ("line_items[0][price]", price_id),
            ("line_items[0][quantity]", "1"),
        ];

        let resp = self.http
            .post(&url)
            .bearer_auth(&self.secret_key)
            .form(&form)
            .send()
            .await?
            .error_for_status()?;

        let session: CheckoutSession = resp.json().await?;
        tracing::info!(session_id = %session.id, "Stripe Checkout session created");
        Ok(session)
    }

    /// Open the Stripe billing portal for an existing customer.
    pub async fn create_portal_session(
        &self,
        customer_id: &str,
        return_url: &str,
    ) -> Result<String> {
        let url = format!("{STRIPE_API}/billing_portal/sessions");
        
        let form = [
            ("customer", customer_id),
            ("return_url", return_url),
        ];

        let resp = self.http
            .post(&url)
            .bearer_auth(&self.secret_key)
            .form(&form)
            .send()
            .await?
            .error_for_status()?;

        #[derive(Deserialize)]
        struct PortalSession {
            url: String,
        }
        
        let session: PortalSession = resp.json().await?;
        tracing::info!(customer_id, "Stripe portal session created");
        Ok(session.url)
    }
}

/// Stripe webhook event types Phantom cares about.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StripeWebhookEvent {
    #[serde(rename = "customer.subscription.updated")]
    SubscriptionUpdated { data: serde_json::Value },
    #[serde(rename = "invoice.paid")]
    InvoicePaid { data: serde_json::Value },
    #[serde(rename = "invoice.payment_failed")]
    InvoicePaymentFailed { data: serde_json::Value },
    #[serde(other)]
    Unknown,
}
