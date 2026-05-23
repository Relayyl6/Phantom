//! Supabase client — auth, storage, and Postgres REST.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};


#[derive(Debug, Deserialize)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub user: AuthUser,
}

#[derive(Debug, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
}

pub struct SupabaseClient {
    project_url: String,
    anon_key: String,
    http: Client,
    /// Set after successful sign-in.
    pub access_token: Option<String>,
}

impl SupabaseClient {
    pub fn new(project_url: String, anon_key: String) -> Self {
        Self {
            project_url,
            anon_key,
            http: Client::new(),
            access_token: None,
        }
    }

    pub fn with_defaults() -> Self {
        let project_url = std::env::var("SUPABASE_URL")
            .unwrap_or_else(|_| "https://YOUR_PROJECT.supabase.co".to_owned());
        let anon_key = std::env::var("SUPABASE_ANON_KEY")
            .unwrap_or_else(|_| "YOUR_ANON_KEY".to_owned());
        
        Self::new(project_url, anon_key)
    }

    fn auth_header(&self) -> String {
        self.access_token
            .as_deref()
            .map(|t| format!("Bearer {t}"))
            .unwrap_or_else(|| format!("Bearer {}", self.anon_key))
    }

    /// Sign in with email + password.
    pub async fn sign_in(&mut self, email: &str, password: &str) -> Result<AuthUser> {
        #[derive(Serialize)]
        struct Creds<'a> { email: &'a str, password: &'a str }

        let resp: AuthSession = self.http
            .post(format!("{}/auth/v1/token?grant_type=password", self.project_url))
            .header("apikey", &self.anon_key)
            .json(&Creds { email, password })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        self.access_token = Some(resp.access_token);
        Ok(resp.user)
    }

    /// Upload a file to a Supabase Storage bucket.
    /// Returns the public URL of the uploaded object.
    pub async fn upload_file(
        &self,
        bucket: &str,
        object_path: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String> {
        let url = format!(
            "{}/storage/v1/object/{bucket}/{object_path}",
            self.project_url
        );
        self.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("apikey", &self.anon_key)
            .header("Content-Type", content_type)
            .body(data)
            .send()
            .await?
            .error_for_status()?;

        Ok(format!(
            "{}/storage/v1/object/public/{bucket}/{object_path}",
            self.project_url
        ))
    }

    /// Create a signed (time-limited) URL for a private storage object.
    pub async fn create_signed_url(
        &self,
        bucket: &str,
        object_path: &str,
        expires_in_secs: u64,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct Body<'a> { #[serde(rename = "expiresIn")] expires_in: u64, path: &'a str }
        #[derive(Deserialize)]
        struct Resp { #[serde(rename = "signedURL")] signed_url: String }

        let url = format!(
            "{}/storage/v1/object/sign/{bucket}/{object_path}",
            self.project_url
        );
        let resp: Resp = self.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("apikey", &self.anon_key)
            .json(&Body { expires_in: expires_in_secs, path: object_path })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(format!("{}{}", self.project_url, resp.signed_url))
    }

    /// Perform a generic GET request against a Supabase PostgREST endpoint.
    pub async fn get_rest<T: serde::de::DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let url = format!("{}{}", self.project_url, endpoint);
        let resp = self.http
            .get(&url)
            .header("Authorization", self.auth_header())
            .header("apikey", &self.anon_key)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Perform a generic POST request against a Supabase PostgREST endpoint.
    pub async fn post_rest<T: serde::de::DeserializeOwned, B: Serialize>(&self, endpoint: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.project_url, endpoint);
        let resp = self.http
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("apikey", &self.anon_key)
            .header("Prefer", "return=representation")
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        
        // PostgREST returns an array for inserts with return=representation.
        // We attempt to deserialize it, or if it's a single object, we handle it.
        // For simplicity, we just deserialize T. If the caller expects a Vec, they pass Vec<T>.
        Ok(resp.json().await?)
    }
}
