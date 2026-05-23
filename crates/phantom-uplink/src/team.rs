//! Team workspaces and comments — Phase 4 feature.
//!
//! Provides API structures and clients for Team tier features, including
//! shared workspaces, role-based access, and time-coded video comments.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::supabase_client::SupabaseClient;

/// A shared team workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub owner_id: String,
}

/// A member of a team workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMember {
    pub workspace_id: Uuid,
    pub user_id: String,
    pub role: String, // "admin", "member", "viewer"
}

/// A time-coded comment on a recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: Uuid,
    pub recording_id: Uuid,
    pub user_id: String,
    pub text: String,
    /// The timestamp in the video this comment refers to (milliseconds).
    pub timestamp_ms: Option<u64>,
    pub created_at: DateTime<Utc>,
}

pub struct TeamClient<'a> {
    supabase: &'a SupabaseClient,
}

impl<'a> TeamClient<'a> {
    pub fn new(supabase: &'a SupabaseClient) -> Self {
        Self { supabase }
    }

    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        self.supabase.get_rest("/rest/v1/workspaces").await
    }

    pub async fn get_comments(&self, recording_id: Uuid) -> Result<Vec<Comment>> {
        let endpoint = format!("/rest/v1/comments?recording_id=eq.{}", recording_id);
        self.supabase.get_rest(&endpoint).await
    }

    pub async fn post_comment(&self, recording_id: Uuid, text: String, timestamp_ms: Option<u64>) -> Result<Comment> {
        #[derive(Serialize)]
        struct NewComment {
            recording_id: Uuid,
            text: String,
            timestamp_ms: Option<u64>,
        }
        
        let body = NewComment {
            recording_id,
            text,
            timestamp_ms,
        };

        // PostgREST with return=representation returns an array of inserted rows.
        let mut inserted: Vec<Comment> = self.supabase.post_rest("/rest/v1/comments", &body).await?;
        inserted.pop().ok_or_else(|| anyhow::anyhow!("Failed to return inserted comment"))
    }
}
