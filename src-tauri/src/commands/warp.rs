//! One-click Cloudflare WARP registration: calls the `warp` crate's real,
//! anonymous device-registration flow and turns the result directly into a
//! WireGuard `ServerConfig`, appended to the persisted config -- same
//! save-then-return-clone shape as `commands::config::servers_add` and
//! `commands::subscription::subscription_import`. See `docs/ipc-contract.md`'s
//! "Cloudflare WARP" section for the user-facing behavior and known
//! limitations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use shared_types::{AppError, AppResult, Protocol, ServerConfig, ServerSource, UserConfig};
use tauri::{AppHandle, State};

use crate::state::{save_persisted_config, AppState};

/// Monotonic counter mixed into generated ids alongside a pid+nanos
/// timestamp -- same lightweight-uniqueness convention as
/// `subscription::parse::generate_id` (see that module's doc comment); a
/// dedicated counter isn't strictly needed here since only one id is minted
/// per `warp_register` call (unlike a subscription batch import), but
/// mirroring the exact convention keeps id shapes consistent across the
/// codebase's server-generating code paths.
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn generate_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("warp-{}-{nanos}-{seq}", std::process::id())
}

/// The UI keys server identity off `ServerConfig.id` (see `ServersView.tsx`'s
/// `key={server.id}` and `deleteServer(id)`), not `.name` -- names are
/// display-only and never used for lookups. Still, silently producing two
/// servers both literally named "Cloudflare WARP" reads as a bug to a user
/// registering more than once, so repeat registrations get a
/// `" (2)"`/`" (3)"`/... suffix instead of colliding on the display name.
fn unique_name(existing: &[ServerConfig], base: &str) -> String {
    if !existing.iter().any(|s| s.name == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base} ({n})");
        if !existing.iter().any(|s| s.name == candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[tauri::command]
pub async fn warp_register(app: AppHandle, state: State<'_, AppState>) -> AppResult<UserConfig> {
    // Real network call happens before any lock is taken -- mirrors
    // `subscription_import`'s `fetch_subscription(...).await` followed by a
    // synchronous lock/mutate/drop/save, since a `std::sync::MutexGuard`
    // can't be held across an `.await` point.
    let registration = warp::register()
        .await
        .map_err(|e| AppError::new("warp_registration_failed", e.to_string()))?;

    let mut config = state.config.lock().unwrap();
    let name = unique_name(&config.servers, "Cloudflare WARP");
    let server = ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Wireguard,
        address: registration.endpoint_address,
        port: registration.endpoint_port,
        uuid: None,
        password: None,
        encryption: None,
        flow: None,
        tls: None,
        wireguard_private_key: Some(registration.private_key),
        wireguard_peer_public_key: Some(registration.peer_public_key),
        wireguard_pre_shared_key: None,
        wireguard_local_address: Some(registration.local_address_v4),
        // Not from a subscription -- registered directly against
        // Cloudflare's own API, same category as a hand-typed server.
        source: ServerSource::Manual,
    };
    config.servers.push(server);
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}
