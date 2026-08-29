//! sing-box JSON config generation from `ServerConfig` + `UserConfig`.
//! Mirrors `singbox-config-helpers.ts` / `singbox-outbound-builder.ts` /
//! `singbox-route-builder.ts` / `singbox-dns-builder.ts` /
//! `singbox-inbounds-builder.ts` in the Electron codebase (see repo
//! `FlowZ/src/main/services/`) — port that logic here, scoped to the
//! protocols in `shared_types::Protocol` for now.

use shared_types::ServerConfig;

pub fn build_outbound(_server: &ServerConfig) -> serde_json::Value {
    unimplemented!("core-manager: build_outbound")
}
