//! sing-box JSON config generation from `ServerConfig` + `UserConfig`.
//! Mirrors `singbox-config-helpers.ts` / `singbox-outbound-builder.ts` /
//! `singbox-route-builder.ts` / `singbox-dns-builder.ts` /
//! `singbox-inbounds-builder.ts` in the Electron codebase (see repo
//! `FlowZ/src/main/services/`) — port that logic here, scoped to the
//! protocols in `shared_types::Protocol` for now.
//!
//! MVP scope (see `docs/ipc-contract.md` + the core-manager task brief):
//! one local `mixed` inbound (HTTP+SOCKS on one port, loopback only), one
//! outbound matching the server's protocol plus a `direct` outbound, and a
//! route whose `final` sends everything through the proxy outbound. No
//! DNS config, no TUN inbound, no rule-based routing — those are phase 2.

use std::net::Ipv4Addr;

use serde_json::{json, Value};
use shared_types::{Protocol, ServerConfig, TlsConfig};

/// Tag of the generated proxy outbound — `route.final` points at this.
pub const PROXY_OUTBOUND_TAG: &str = "proxy";
/// Tag of the generated `direct` outbound.
pub const DIRECT_OUTBOUND_TAG: &str = "direct";
/// Tag of the generated local `mixed` inbound.
pub const MIXED_INBOUND_TAG: &str = "mixed-in";

/// Builds the `tls` object for a proxy outbound, or `None` when the server
/// has no TLS configured / TLS disabled. Scoped-down relative to the
/// Electron builder (no uTLS fingerprint, ALPN, ECH, ...) — just enough for
/// a real handshake: `server_name`, `insecure`, and Reality when the server
/// carries Reality settings.
fn build_tls(tls: &Option<TlsConfig>, fallback_server_name: &str) -> Option<Value> {
    let tls = tls.as_ref()?;
    if !tls.enabled {
        return None;
    }

    let mut obj = json!({
        "enabled": true,
        "server_name": tls.server_name.clone().unwrap_or_else(|| fallback_server_name.to_string()),
        "insecure": tls.insecure,
    });

    if let (Some(public_key), Some(short_id)) = (&tls.reality_public_key, &tls.reality_short_id) {
        obj["reality"] = json!({
            "enabled": true,
            "public_key": public_key,
            "short_id": short_id,
        });
        // sing-box requires uTLS whenever Reality is enabled ("uTLS is
        // required by reality client") — Reality's whole premise is
        // mimicking a real site's TLS fingerprint, so this isn't optional
        // the way it is for plain TLS. `chrome` matches the Electron
        // builder's Reality default (see singbox-outbound-builder.ts).
        obj["utls"] = json!({
            "enabled": true,
            "fingerprint": "chrome",
        });
    }

    Some(obj)
}

/// Builds the sing-box outbound object for `server`, tagged
/// `PROXY_OUTBOUND_TAG`. Field mapping per protocol (see
/// `singbox-outbound-builder.ts::buildProxyOutbound` for the full-featured
/// reference this is scoped down from):
/// - `vless`: `uuid` (+ `flow` when set)
/// - `vmess`: `uuid`, `security` (from `encryption`, default `auto`), `alter_id: 0`
/// - `trojan`: `password`
/// - `shadowsocks`: `method` (from `encryption`), `password`
pub fn build_outbound(server: &ServerConfig) -> Value {
    let mut outbound = match server.protocol {
        Protocol::Vless => {
            let mut o = json!({
                "type": "vless",
                "tag": PROXY_OUTBOUND_TAG,
                "server": server.address,
                "server_port": server.port,
                "uuid": server.uuid.clone().unwrap_or_default(),
            });
            if let Some(flow) = server.flow.as_deref() {
                if !flow.is_empty() {
                    o["flow"] = json!(flow);
                }
            }
            o
        }
        Protocol::Vmess => json!({
            "type": "vmess",
            "tag": PROXY_OUTBOUND_TAG,
            "server": server.address,
            "server_port": server.port,
            "uuid": server.uuid.clone().unwrap_or_default(),
            "security": server.encryption.clone().unwrap_or_else(|| "auto".to_string()),
            "alter_id": 0,
        }),
        Protocol::Trojan => json!({
            "type": "trojan",
            "tag": PROXY_OUTBOUND_TAG,
            "server": server.address,
            "server_port": server.port,
            "password": server.password.clone().unwrap_or_default(),
        }),
        Protocol::Shadowsocks => json!({
            "type": "shadowsocks",
            "tag": PROXY_OUTBOUND_TAG,
            "server": server.address,
            "server_port": server.port,
            "method": server.encryption.clone().unwrap_or_else(|| "aes-256-gcm".to_string()),
            "password": server.password.clone().unwrap_or_default(),
        }),
    };

    // sing-box's shadowsocks outbound has no top-level `tls` field (TLS-ing
    // shadowsocks goes through a plugin, out of MVP scope) — only attach
    // TLS for the other three protocols.
    if !matches!(server.protocol, Protocol::Shadowsocks) {
        if let Some(tls) = build_tls(&server.tls, &server.address) {
            outbound["tls"] = tls;
        }
    }

    outbound
}

/// Builds the local `mixed` inbound: HTTP+SOCKS on one port, 127.0.0.1 only.
/// No LAN exposure, no sniffing, no TUN — MVP is a single-outbound loopback
/// proxy; `port` is whatever the caller picked (typically an OS-assigned
/// ephemeral port, see `CoreManager::start`).
pub fn build_inbound(port: u16) -> Value {
    json!({
        "type": "mixed",
        "tag": MIXED_INBOUND_TAG,
        "listen": Ipv4Addr::LOCALHOST.to_string(),
        "listen_port": port,
    })
}

/// Assembles the full sing-box config for a single-server MVP run: one
/// `mixed` inbound on `inbound_port`, the server's proxy outbound plus a
/// `direct` outbound, and `route.final` pointed at the proxy so all traffic
/// through the inbound goes out via the configured server. No DNS block,
/// no rules, no TUN — sing-box's own defaults cover DNS resolution for this
/// scope.
pub fn build_config(server: &ServerConfig, inbound_port: u16) -> Value {
    json!({
        "log": {
            "level": "info",
            "timestamp": true,
        },
        "inbounds": [build_inbound(inbound_port)],
        "outbounds": [
            build_outbound(server),
            {
                "type": "direct",
                "tag": DIRECT_OUTBOUND_TAG,
            },
        ],
        "route": {
            "final": PROXY_OUTBOUND_TAG,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::TlsConfig;

    fn base_server(protocol: Protocol) -> ServerConfig {
        ServerConfig {
            id: "srv-1".into(),
            name: "test".into(),
            protocol,
            address: "example.com".into(),
            port: 443,
            uuid: Some("uuid-value".into()),
            password: Some("pw".into()),
            encryption: None,
            flow: None,
            tls: None,
        }
    }

    #[test]
    fn vless_outbound_has_required_fields() {
        let server = base_server(Protocol::Vless);
        let o = build_outbound(&server);
        assert_eq!(o["type"], "vless");
        assert_eq!(o["tag"], PROXY_OUTBOUND_TAG);
        assert_eq!(o["server"], "example.com");
        assert_eq!(o["server_port"], 443);
        assert_eq!(o["uuid"], "uuid-value");
        assert!(o.get("flow").is_none());
    }

    #[test]
    fn vless_flow_only_emitted_when_present() {
        let mut server = base_server(Protocol::Vless);
        server.flow = Some("xtls-rprx-vision".into());
        let o = build_outbound(&server);
        assert_eq!(o["flow"], "xtls-rprx-vision");
    }

    #[test]
    fn vmess_defaults_security_to_auto() {
        let server = base_server(Protocol::Vmess);
        let o = build_outbound(&server);
        assert_eq!(o["type"], "vmess");
        assert_eq!(o["security"], "auto");
        assert_eq!(o["alter_id"], 0);
    }

    #[test]
    fn trojan_uses_password() {
        let server = base_server(Protocol::Trojan);
        let o = build_outbound(&server);
        assert_eq!(o["type"], "trojan");
        assert_eq!(o["password"], "pw");
    }

    #[test]
    fn shadowsocks_uses_method_and_password_no_tls() {
        let mut server = base_server(Protocol::Shadowsocks);
        server.encryption = Some("chacha20-ietf-poly1305".into());
        server.tls = Some(TlsConfig {
            enabled: true,
            server_name: None,
            insecure: false,
            reality_public_key: None,
            reality_short_id: None,
        });
        let o = build_outbound(&server);
        assert_eq!(o["type"], "shadowsocks");
        assert_eq!(o["method"], "chacha20-ietf-poly1305");
        assert_eq!(o["password"], "pw");
        assert!(o.get("tls").is_none());
    }

    #[test]
    fn tls_falls_back_to_server_address_for_sni() {
        let mut server = base_server(Protocol::Trojan);
        server.tls = Some(TlsConfig {
            enabled: true,
            server_name: None,
            insecure: false,
            reality_public_key: None,
            reality_short_id: None,
        });
        let o = build_outbound(&server);
        assert_eq!(o["tls"]["enabled"], true);
        assert_eq!(o["tls"]["server_name"], "example.com");
        assert_eq!(o["tls"]["insecure"], false);
    }

    #[test]
    fn tls_disabled_omits_tls_block() {
        let mut server = base_server(Protocol::Trojan);
        server.tls = Some(TlsConfig {
            enabled: false,
            server_name: None,
            insecure: false,
            reality_public_key: None,
            reality_short_id: None,
        });
        let o = build_outbound(&server);
        assert!(o.get("tls").is_none());
    }

    #[test]
    fn reality_settings_are_emitted() {
        let mut server = base_server(Protocol::Vless);
        server.tls = Some(TlsConfig {
            enabled: true,
            server_name: Some("real.example".into()),
            insecure: false,
            reality_public_key: Some("pubkey".into()),
            reality_short_id: Some("abcd".into()),
        });
        let o = build_outbound(&server);
        assert_eq!(o["tls"]["reality"]["enabled"], true);
        assert_eq!(o["tls"]["reality"]["public_key"], "pubkey");
        assert_eq!(o["tls"]["reality"]["short_id"], "abcd");
        // sing-box rejects Reality without uTLS ("uTLS is required by
        // reality client") — verified against the real binary's `check`.
        assert_eq!(o["tls"]["utls"]["enabled"], true);
    }

    #[test]
    fn full_config_has_mixed_inbound_and_final_route() {
        let server = base_server(Protocol::Trojan);
        let cfg = build_config(&server, 12345);
        assert_eq!(cfg["inbounds"][0]["type"], "mixed");
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
        assert_eq!(cfg["inbounds"][0]["listen_port"], 12345);
        assert_eq!(cfg["outbounds"].as_array().unwrap().len(), 2);
        assert_eq!(cfg["outbounds"][1]["type"], "direct");
        assert_eq!(cfg["route"]["final"], PROXY_OUTBOUND_TAG);
    }
}
