//! sing-box JSON config generation from `ServerConfig` + `UserConfig`.
//! Mirrors `singbox-config-helpers.ts` / `singbox-outbound-builder.ts` /
//! `singbox-route-builder.ts` / `singbox-dns-builder.ts` /
//! `singbox-inbounds-builder.ts` in the Electron codebase (see repo
//! `FlowZ/src/main/services/`) — port that logic here, scoped to the
//! protocols in `shared_types::Protocol` for now.
//!
//! MVP scope (see `docs/ipc-contract.md` + the core-manager task brief):
//! one local `mixed` inbound (HTTP+SOCKS on one port, loopback only), one
//! outbound matching the server's protocol plus a `direct` outbound (and a
//! `block` outbound when a rule needs one), a `route.rules` array built from
//! `UserConfig.rules` (domain/domain-suffix/domain-keyword/IP-CIDR/
//! process-name matching only, no compound conditions, no GeoIP/GeoSite
//! rule-set files), and a route whose `final` sends anything no rule matched
//! through the proxy outbound. No DNS config, no TUN inbound configured here
//! (see `tun.rs`).

use std::net::Ipv4Addr;

use serde_json::{json, Value};
use shared_types::{Protocol, RoutingRule, RuleMatchType, RuleOutbound, ServerConfig, TlsConfig};

use crate::tun;

/// Default TUN interface name, used when nothing more specific is wired up
/// (e.g. per-platform naming/collision-avoidance) — sing-box creates the
/// interface under this name on all three desktop platforms it supports
/// (`utun`-prefixed real name substitution happens internally on macOS
/// regardless of what's requested here).
const DEFAULT_TUN_INTERFACE_NAME: &str = "ferroflow-tun0";

/// Tag of the generated proxy outbound — `route.final` points at this.
pub const PROXY_OUTBOUND_TAG: &str = "proxy";
/// Tag of the generated `direct` outbound.
pub const DIRECT_OUTBOUND_TAG: &str = "direct";
/// Tag of the generated local `mixed` inbound.
pub const MIXED_INBOUND_TAG: &str = "mixed-in";
/// Tag of the generated `block` outbound — only emitted when at least one
/// enabled rule references it (see `build_config_with_inbound`).
pub const BLOCK_OUTBOUND_TAG: &str = "block";

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

/// Maps a `RuleMatchType` to the sing-box route-rule JSON field name it
/// corresponds to.
fn match_field_name(match_type: RuleMatchType) -> &'static str {
    match match_type {
        RuleMatchType::Domain => "domain",
        RuleMatchType::DomainSuffix => "domain_suffix",
        RuleMatchType::DomainKeyword => "domain_keyword",
        RuleMatchType::IpCidr => "ip_cidr",
        RuleMatchType::ProcessName => "process_name",
    }
}

/// Maps a `RuleOutbound` to the tag of the outbound it should route to.
fn outbound_tag(outbound: RuleOutbound) -> &'static str {
    match outbound {
        RuleOutbound::Proxy => PROXY_OUTBOUND_TAG,
        RuleOutbound::Direct => DIRECT_OUTBOUND_TAG,
        RuleOutbound::Block => BLOCK_OUTBOUND_TAG,
    }
}

/// Builds the `route.rules` array from `rules`: disabled rules and rules
/// with no match values are skipped (an empty match-field array is
/// meaningless to sing-box, and a disabled rule shouldn't affect routing at
/// all). Each `RoutingRule` sets exactly one match field — this codebase
/// doesn't build compound-condition rules — plus an `outbound` tag. List
/// order is preserved, matching sing-box's top-to-bottom first-match-wins
/// evaluation.
pub fn build_route_rules(rules: &[RoutingRule]) -> Vec<Value> {
    rules
        .iter()
        .filter(|rule| rule.enabled && !rule.values.is_empty())
        .map(|rule| {
            json!({
                match_field_name(rule.match_type): rule.values,
                "outbound": outbound_tag(rule.outbound),
            })
        })
        .collect()
}

/// Assembles the full sing-box config for a single-server MVP run: one
/// `mixed` inbound on `inbound_port`, the server's proxy outbound plus a
/// `direct` outbound (and a `block` outbound when needed), `route.rules`
/// built from `rules`, and `route.final` pointed at the proxy as the
/// fallback when no rule matches. No DNS block, no TUN — sing-box's own
/// defaults cover DNS resolution for this scope.
///
/// `clash_api_port` enables sing-box's built-in Clash API
/// (`experimental.clash_api.external_controller`) on `127.0.0.1:<port>`,
/// giving `core-manager` a way to query live connections/traffic totals
/// (see `crate::clash_api`) independent of which inbound is active.
pub fn build_config(
    server: &ServerConfig,
    inbound_port: u16,
    rules: &[RoutingRule],
    clash_api_port: u16,
) -> Value {
    build_config_with_inbound(server, build_inbound(inbound_port), rules, clash_api_port)
}

/// Assembles the full sing-box config for a single-server TUN-mode run: one
/// `tun` inbound (see `tun::build_tun_inbound`) in place of the local
/// `mixed` inbound, same outbounds/route as `build_config`. This is the
/// config handed to the privileged helper (`HelperClient::start`) — a plain
/// unprivileged process can't create a TUN interface, hence routing through
/// the helper for this mode instead of `process::ProcessHandle`.
///
/// `clash_api_port` — see `build_config`'s doc comment; TUN mode gets the
/// same Clash API block since traffic visibility shouldn't depend on which
/// inbound is active.
pub fn build_tun_config(server: &ServerConfig, rules: &[RoutingRule], clash_api_port: u16) -> Value {
    build_config_with_inbound(
        server,
        tun::build_tun_inbound(DEFAULT_TUN_INTERFACE_NAME),
        rules,
        clash_api_port,
    )
}

/// Shared assembly: `inbound` is the only thing that differs between the
/// mixed-proxy and TUN config shapes — outbounds/route are otherwise
/// identical. A `block` outbound is only added when at least one enabled
/// rule actually references it, so we never emit an outbound nothing points
/// at. `clash_api_port` is always set (every run gets a Clash API listener,
/// regardless of mode) — see `build_config`'s doc comment.
fn build_config_with_inbound(
    server: &ServerConfig,
    inbound: Value,
    rules: &[RoutingRule],
    clash_api_port: u16,
) -> Value {
    let mut outbounds = vec![
        build_outbound(server),
        json!({
            "type": "direct",
            "tag": DIRECT_OUTBOUND_TAG,
        }),
    ];

    let needs_block = rules.iter().any(|r| r.enabled && !r.values.is_empty() && r.outbound == RuleOutbound::Block);
    if needs_block {
        outbounds.push(json!({
            "type": "block",
            "tag": BLOCK_OUTBOUND_TAG,
        }));
    }

    json!({
        "log": {
            "level": "info",
            "timestamp": true,
        },
        "inbounds": [inbound],
        "outbounds": outbounds,
        "route": {
            "rules": build_route_rules(rules),
            "final": PROXY_OUTBOUND_TAG,
        },
        "experimental": {
            "clash_api": {
                "external_controller": format!("{}:{}", Ipv4Addr::LOCALHOST, clash_api_port),
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::TlsConfig;
    use std::path::PathBuf;

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
        let cfg = build_config(&server, 12345, &[], 9999);
        assert_eq!(cfg["inbounds"][0]["type"], "mixed");
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
        assert_eq!(cfg["inbounds"][0]["listen_port"], 12345);
        assert_eq!(cfg["outbounds"].as_array().unwrap().len(), 2);
        assert_eq!(cfg["outbounds"][1]["type"], "direct");
        assert_eq!(cfg["route"]["final"], PROXY_OUTBOUND_TAG);
        assert_eq!(cfg["experimental"]["clash_api"]["external_controller"], "127.0.0.1:9999");
    }

    #[test]
    fn full_tun_config_has_tun_inbound_and_final_route() {
        let server = base_server(Protocol::Trojan);
        let cfg = build_tun_config(&server, &[], 9999);
        assert_eq!(cfg["inbounds"][0]["type"], "tun");
        assert_eq!(cfg["inbounds"][0]["interface_name"], DEFAULT_TUN_INTERFACE_NAME);
        assert_eq!(cfg["outbounds"].as_array().unwrap().len(), 2);
        assert_eq!(cfg["outbounds"][1]["type"], "direct");
        assert_eq!(cfg["route"]["final"], PROXY_OUTBOUND_TAG);
        assert_eq!(cfg["experimental"]["clash_api"]["external_controller"], "127.0.0.1:9999");
    }

    fn rule(match_type: RuleMatchType, values: &[&str], outbound: RuleOutbound) -> RoutingRule {
        RoutingRule {
            id: "rule-1".into(),
            name: "test rule".into(),
            enabled: true,
            match_type,
            values: values.iter().map(|s| s.to_string()).collect(),
            outbound,
        }
    }

    #[test]
    fn route_rule_domain_maps_to_domain_field() {
        let r = rule(RuleMatchType::Domain, &["example.com"], RuleOutbound::Direct);
        let rules = build_route_rules(&[r]);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["domain"], json!(["example.com"]));
        assert_eq!(rules[0]["outbound"], DIRECT_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_domain_suffix_maps_to_domain_suffix_field() {
        let r = rule(RuleMatchType::DomainSuffix, &[".cn"], RuleOutbound::Direct);
        let rules = build_route_rules(&[r]);
        assert_eq!(rules[0]["domain_suffix"], json!([".cn"]));
        assert_eq!(rules[0]["outbound"], DIRECT_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_domain_keyword_maps_to_domain_keyword_field() {
        let r = rule(RuleMatchType::DomainKeyword, &["ads"], RuleOutbound::Block);
        let rules = build_route_rules(&[r]);
        assert_eq!(rules[0]["domain_keyword"], json!(["ads"]));
        assert_eq!(rules[0]["outbound"], BLOCK_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_ip_cidr_maps_to_ip_cidr_field() {
        let r = rule(RuleMatchType::IpCidr, &["10.0.0.0/8"], RuleOutbound::Block);
        let rules = build_route_rules(&[r]);
        assert_eq!(rules[0]["ip_cidr"], json!(["10.0.0.0/8"]));
        assert_eq!(rules[0]["outbound"], BLOCK_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_process_name_maps_to_process_name_field() {
        let r = rule(RuleMatchType::ProcessName, &["chrome.exe"], RuleOutbound::Proxy);
        let rules = build_route_rules(&[r]);
        assert_eq!(rules[0]["process_name"], json!(["chrome.exe"]));
        assert_eq!(rules[0]["outbound"], PROXY_OUTBOUND_TAG);
    }

    #[test]
    fn disabled_rule_is_excluded() {
        let mut r = rule(RuleMatchType::Domain, &["example.com"], RuleOutbound::Direct);
        r.enabled = false;
        let rules = build_route_rules(&[r]);
        assert!(rules.is_empty());
    }

    #[test]
    fn empty_values_rule_is_excluded() {
        let r = rule(RuleMatchType::Domain, &[], RuleOutbound::Direct);
        let rules = build_route_rules(&[r]);
        assert!(rules.is_empty());
    }

    #[test]
    fn block_outbound_absent_when_no_block_rules() {
        let server = base_server(Protocol::Trojan);
        let r = rule(RuleMatchType::Domain, &["example.com"], RuleOutbound::Direct);
        let cfg = build_config(&server, 12345, &[r], 9999);
        assert_eq!(cfg["outbounds"].as_array().unwrap().len(), 2);
        assert!(cfg["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["type"] != "block"));
    }

    #[test]
    fn block_outbound_present_when_enabled_block_rule_exists() {
        let server = base_server(Protocol::Trojan);
        let r = rule(RuleMatchType::IpCidr, &["10.0.0.0/8"], RuleOutbound::Block);
        let cfg = build_config(&server, 12345, &[r], 9999);
        let outbounds = cfg["outbounds"].as_array().unwrap();
        assert_eq!(outbounds.len(), 3);
        assert!(outbounds.iter().any(|o| o["type"] == "block" && o["tag"] == BLOCK_OUTBOUND_TAG));
    }

    #[test]
    fn block_outbound_absent_when_block_rule_disabled() {
        let server = base_server(Protocol::Trojan);
        let mut r = rule(RuleMatchType::IpCidr, &["10.0.0.0/8"], RuleOutbound::Block);
        r.enabled = false;
        let cfg = build_config(&server, 12345, &[r], 9999);
        assert_eq!(cfg["outbounds"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn route_rules_appear_in_generated_config() {
        let server = base_server(Protocol::Trojan);
        let r = rule(RuleMatchType::DomainSuffix, &[".cn"], RuleOutbound::Direct);
        let cfg = build_config(&server, 12345, &[r], 9999);
        assert_eq!(cfg["route"]["rules"].as_array().unwrap().len(), 1);
        assert_eq!(cfg["route"]["rules"][0]["domain_suffix"], json!([".cn"]));
        assert_eq!(cfg["route"]["final"], PROXY_OUTBOUND_TAG);
    }

    /// Real validation against an actual sing-box binary's `check -c <file>`
    /// subcommand — the single most important check for this feature, since
    /// a JSON shape sing-box's schema rejects (wrong field name, wrong type,
    /// a `block` outbound referenced but never emitted, ...) only surfaces
    /// this way, not via any `cargo test` assertion above. Builds a config
    /// with one rule of every `RuleMatchType`/`RuleOutbound` combination
    /// (including a `Block`-outbound rule, to confirm the conditionally-added
    /// `block` outbound is both present and validly referenced) and asks the
    /// real binary to validate it. Not run by default (`#[ignore]`), same
    /// convention as `CoreManager`'s `real_singbox_local_backend_*` test —
    /// needs a real binary at `<workspace root>/.dev-bin/sing-box[.exe]`.
    /// Run manually with:
    /// `cargo test -p core-manager --all-targets -- --ignored real_singbox`
    #[test]
    #[ignore = "needs a real sing-box binary at <workspace root>/.dev-bin/"]
    fn real_singbox_check_accepts_config_with_all_rule_kinds() {
        use std::process::Command;

        let binary_name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
        let binary =
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.dev-bin")).join(binary_name);
        assert!(binary.is_file(), "expected a real sing-box binary at {}", binary.display());

        fn r(id: &str, match_type: RuleMatchType, values: &[&str], outbound: RuleOutbound) -> RoutingRule {
            RoutingRule {
                id: id.into(),
                name: id.into(),
                enabled: true,
                match_type,
                values: values.iter().map(|s| s.to_string()).collect(),
                outbound,
            }
        }

        let rules = vec![
            r("r-domain", RuleMatchType::Domain, &["example.com"], RuleOutbound::Direct),
            r("r-suffix", RuleMatchType::DomainSuffix, &[".cn"], RuleOutbound::Direct),
            r("r-keyword", RuleMatchType::DomainKeyword, &["ads"], RuleOutbound::Block),
            r("r-cidr", RuleMatchType::IpCidr, &["10.0.0.0/8"], RuleOutbound::Block),
            r("r-process", RuleMatchType::ProcessName, &["chrome.exe"], RuleOutbound::Proxy),
        ];

        let server = base_server(Protocol::Trojan);
        let cfg = build_config(&server, 12345, &rules, 9999);

        // Sanity-check the block outbound really is present before handing
        // this to sing-box, so a failed `check` clearly means "sing-box
        // rejected the shape" rather than "we forgot to build it".
        assert!(cfg["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["type"] == "block"));

        let mut path = std::env::temp_dir();
        path.push(format!("ferroflow-config-rules-check-{}.json", std::process::id()));
        std::fs::write(&path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();

        let output = Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .expect("failed to run sing-box check");

        let _ = std::fs::remove_file(&path);

        assert!(
            output.status.success(),
            "sing-box check rejected the generated config.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Confirms sing-box's real schema validator accepts the
    /// `experimental.clash_api` block this module now emits on every config
    /// (both `build_config` and `build_tun_config` — see their doc
    /// comments). Same convention as
    /// `real_singbox_check_accepts_config_with_all_rule_kinds`: not run by
    /// default (`#[ignore]`), needs a real binary at
    /// `<workspace root>/.dev-bin/sing-box[.exe]`. Run manually with:
    /// `cargo test -p core-manager --all-targets -- --ignored real_singbox`
    #[test]
    #[ignore = "needs a real sing-box binary at <workspace root>/.dev-bin/"]
    fn real_singbox_check_accepts_config_with_clash_api() {
        use std::process::Command;

        let binary_name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
        let binary =
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.dev-bin")).join(binary_name);
        assert!(binary.is_file(), "expected a real sing-box binary at {}", binary.display());

        let server = base_server(Protocol::Trojan);
        let cfg = build_config(&server, 12345, &[], 19999);

        // Sanity-check the clash_api block really is present before handing
        // this to sing-box, so a failed `check` clearly means "sing-box
        // rejected the shape" rather than "we forgot to build it".
        assert_eq!(cfg["experimental"]["clash_api"]["external_controller"], "127.0.0.1:19999");

        let mut path = std::env::temp_dir();
        path.push(format!("ferroflow-config-clash-api-check-{}.json", std::process::id()));
        std::fs::write(&path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();

        let output = Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .expect("failed to run sing-box check");

        let _ = std::fs::remove_file(&path);

        assert!(
            output.status.success(),
            "sing-box check rejected the generated config with clash_api enabled.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
