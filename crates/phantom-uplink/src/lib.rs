//! phantom-uplink — Supabase client, share link generation, self-hosted updater.

pub mod share;
pub mod supabase_client;
pub mod updater;
pub mod team;

pub use supabase_client::SupabaseClient;
pub use share::ShareManager;
pub use updater::Updater;
