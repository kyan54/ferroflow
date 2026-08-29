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
//! process-name matching, plus GeoIP/GeoSite `.srs` rule-set references via
//! `RuleMatchType::RuleSet` -- see `build_rule_set_entries`/
//! `build_route_rules` below -- no compound conditions), and a route whose
//! `final` sends anything no rule matched through the proxy outbound. No DNS
//! config, no TUN inbound configured here (see `tun.rs`).

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::PathBuf;

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

/// Builds the sing-box outbound/endpoint object for `server`, tagged
/// `PROXY_OUTBOUND_TAG`. Field mapping per protocol (see
/// `singbox-outbound-builder.ts::buildProxyOutbound` for the full-featured
/// reference this is scoped down from):
/// - `vless`: `uuid` (+ `flow` when set)
/// - `vmess`: `uuid`, `security` (from `encryption`, default `auto`), `alter_id: 0`
/// - `trojan`: `password`
/// - `shadowsocks`: `method` (from `encryption`), `password`
/// - `wireguard`: NOT an outbound on the sing-box versions this app targets
///   -- sing-box deprecated the `wireguard` *outbound* type in 1.11.0 and
///   removed it entirely in 1.13.0 ("WireGuard outbound is deprecated ...
///   and removed in sing-box 1.13.0, use WireGuard endpoint instead",
///   confirmed against the real `.dev-bin/sing-box.exe` binary this
///   workspace validates against, which is 1.13.19). This arm therefore
///   builds a WireGuard *endpoint* object instead: `address` (this app's
///   MVP scope is one address, wrapped into a one-element array -- see
///   `ServerConfig::wireguard_local_address`), `private_key`, and a single
///   `peers` entry carrying `address`/`port` (from `server.address`/
///   `server.port`), `public_key`, `allowed_ips` (hardcoded full-tunnel
///   `0.0.0.0/0`/`::/0` -- no per-server UI for this in MVP scope), and
///   `pre_shared_key` (omitted entirely when not set, not emitted as
///   null/""). The caller (`build_config_with_inbound`) is responsible for
///   placing this object in the config's top-level `endpoints` array rather
///   than `outbounds` -- both are referenced by tag identically from
///   `route.rules`/`route.final`, so `PROXY_OUTBOUND_TAG` still works
///   unchanged as the tag every other protocol uses.
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
        Protocol::Wireguard => {
            let local_address = server.wireguard_local_address.clone().unwrap_or_default();
            let mut peer = json!({
                "address": server.address,
                "port": server.port,
                "public_key": server.wireguard_peer_public_key.clone().unwrap_or_default(),
                "allowed_ips": ["0.0.0.0/0", "::/0"],
            });
            if let Some(psk) = server.wireguard_pre_shared_key.as_deref() {
                if !psk.is_empty() {
                    peer["pre_shared_key"] = json!(psk);
                }
            }
            json!({
                "type": "wireguard",
                "tag": PROXY_OUTBOUND_TAG,
                "address": json!([local_address]),
                "private_key": server.wireguard_private_key.clone().unwrap_or_default(),
                "peers": [peer],
            })
        }
    };

    // sing-box's shadowsocks outbound has no top-level `tls` field (TLS-ing
    // shadowsocks goes through a plugin, out of MVP scope), and WireGuard has
    // its own crypto handshake with no TLS wrapping at all -- only attach TLS
    // for the remaining protocols.
    if !matches!(server.protocol, Protocol::Shadowsocks | Protocol::Wireguard) {
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
/// corresponds to. `RuleSet` is handled separately by `build_route_rules`
/// (its shape is `{"rule_set": [...], ...}` rather than
/// `{"<field>": [...], ...}` with literal values) -- this arm exists only so
/// the match stays exhaustive; it's never actually reached for `RuleSet`.
fn match_field_name(match_type: RuleMatchType) -> &'static str {
    match match_type {
        RuleMatchType::Domain => "domain",
        RuleMatchType::DomainSuffix => "domain_suffix",
        RuleMatchType::DomainKeyword => "domain_keyword",
        RuleMatchType::IpCidr => "ip_cidr",
        RuleMatchType::ProcessName => "process_name",
        RuleMatchType::RuleSet => "rule_set",
    }
}

/// Tag prefix for a generated `route.rule_set` entry, derived from the
/// referenced resource's `RuleResourceInfo.id` -- kept distinct from the
/// bare id so it can never collide with `PROXY_OUTBOUND_TAG`/
/// `DIRECT_OUTBOUND_TAG`/`BLOCK_OUTBOUND_TAG` even if a resource happened to
/// be named e.g. "proxy".
const RULE_SET_TAG_PREFIX: &str = "ruleset-";

fn rule_set_tag(resource_id: &str) -> String {
    format!("{RULE_SET_TAG_PREFIX}{resource_id}")
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
///
/// `RuleMatchType::RuleSet` is the one exception to "match field holds
/// literal values": its `values` are `RuleResourceInfo.id`s, resolved against
/// `resource_paths` (id -> downloaded `.srs` file path, built by the
/// `src-tauri` command layer from `UserConfig.rule_resources` — see
/// `CoreManager::start`). An id with no entry in `resource_paths` (resource
/// never downloaded, or deleted since the rule was created) is skipped with
/// a `tracing::warn!` rather than panicking or producing a broken config; if
/// *every* id in a `RuleSet` rule's `values` is unresolvable, the whole rule
/// is skipped the same way an empty-values rule would be.
pub fn build_route_rules(rules: &[RoutingRule], resource_paths: &HashMap<String, PathBuf>) -> Vec<Value> {
    rules
        .iter()
        .filter(|rule| rule.enabled && !rule.values.is_empty())
        .filter_map(|rule| {
            if rule.match_type == RuleMatchType::RuleSet {
                let tags: Vec<String> = rule
                    .values
                    .iter()
                    .filter_map(|id| {
                        if resource_paths.contains_key(id) {
                            Some(rule_set_tag(id))
                        } else {
                            tracing::warn!(
                                "rule '{}' references rule-set resource '{}' with no known downloaded path -- skipping it",
                                rule.name,
                                id
                            );
                            None
                        }
                    })
                    .collect();

                if tags.is_empty() {
                    tracing::warn!(
                        "rule '{}' has no resolvable rule-set resources -- skipping the whole rule",
                        rule.name
                    );
                    return None;
                }

                Some(json!({
                    "rule_set": tags,
                    "outbound": outbound_tag(rule.outbound),
                }))
            } else {
                Some(json!({
                    match_field_name(rule.match_type): rule.values,
                    "outbound": outbound_tag(rule.outbound),
                }))
            }
        })
        .collect()
}

/// Builds the top-level `route.rule_set` array: one `{"type": "local",
/// "tag": ..., "format": "binary", "path": ...}` entry per **distinct**
/// resource id actually referenced by an *enabled* `RuleSet`-type rule that
/// has a known path in `resource_paths` — an id referenced by a disabled
/// rule, or with no known path, contributes no entry (the corresponding
/// `route.rules` entry from `build_route_rules` already dropped that
/// reference, so emitting an unused `rule_set` entry here would just be
/// dead weight, not a config sing-box would reject — but this module's
/// existing convention is to never emit unused sections, see the `block`
/// outbound in `build_config_with_inbound`). Referenced more than once
/// across multiple rules still yields exactly one entry, tagged the same
/// way both times via `rule_set_tag`.
fn build_rule_set_entries(rules: &[RoutingRule], resource_paths: &HashMap<String, PathBuf>) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for rule in rules.iter().filter(|r| r.enabled && r.match_type == RuleMatchType::RuleSet) {
        for id in &rule.values {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(path) = resource_paths.get(id) {
                entries.push(json!({
                    "type": "local",
                    "tag": rule_set_tag(id),
                    "format": "binary",
                    "path": path.to_string_lossy(),
                }));
            }
        }
    }

    entries
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
    resource_paths: &HashMap<String, PathBuf>,
    clash_api_port: u16,
) -> Value {
    build_config_with_inbound(server, build_inbound(inbound_port), rules, resource_paths, clash_api_port)
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
pub fn build_tun_config(
    server: &ServerConfig,
    rules: &[RoutingRule],
    resource_paths: &HashMap<String, PathBuf>,
    clash_api_port: u16,
) -> Value {
    build_config_with_inbound(
        server,
        tun::build_tun_inbound(DEFAULT_TUN_INTERFACE_NAME),
        rules,
        resource_paths,
        clash_api_port,
    )
}

/// Shared assembly: `inbound` is the only thing that differs between the
/// mixed-proxy and TUN config shapes — outbounds/route are otherwise
/// identical. A `block` outbound is only added when at least one enabled
/// rule actually references it, so we never emit an outbound nothing points
/// at. `clash_api_port` is always set (every run gets a Clash API listener,
/// regardless of mode) — see `build_config`'s doc comment.
///
/// The proxy object from `build_outbound` goes into `outbounds` for every
/// protocol except `Wireguard`, which sing-box 1.13+ requires as a top-level
/// `endpoints` entry instead (see `build_outbound`'s doc comment) — either
/// way it's tagged `PROXY_OUTBOUND_TAG`, so `route.final` and any
/// `RuleOutbound::Proxy` rule resolve to it unchanged regardless of which
/// array it lives in. The `endpoints` key itself is omitted for non-
/// WireGuard servers rather than emitted as `[]`, matching this module's
/// existing convention of not emitting empty/unreferenced sections (see the
/// `block` outbound above).
fn build_config_with_inbound(
    server: &ServerConfig,
    inbound: Value,
    rules: &[RoutingRule],
    resource_paths: &HashMap<String, PathBuf>,
    clash_api_port: u16,
) -> Value {
    let proxy = build_outbound(server);
    let is_wireguard = matches!(server.protocol, Protocol::Wireguard);

    // Mutually exclusive with the `cfg["endpoints"] = ...` assignment below
    // -- `proxy` is moved into exactly one of the two places depending on
    // `is_wireguard`, never both.
    let mut outbounds = Vec::new();
    let mut wireguard_endpoint = None;
    if is_wireguard {
        wireguard_endpoint = Some(proxy);
    } else {
        outbounds.push(proxy);
    }
    outbounds.push(json!({
        "type": "direct",
        "tag": DIRECT_OUTBOUND_TAG,
    }));

    let needs_block = rules.iter().any(|r| r.enabled && !r.values.is_empty() && r.outbound == RuleOutbound::Block);
    if needs_block {
        outbounds.push(json!({
            "type": "block",
            "tag": BLOCK_OUTBOUND_TAG,
        }));
    }

    let rule_set_entries = build_rule_set_entries(rules, resource_paths);
    let mut route = json!({
        "rules": build_route_rules(rules, resource_paths),
        "final": PROXY_OUTBOUND_TAG,
    });
    // Emitted before `rules` is assembled above only conceptually (JSON
    // object key order carries no meaning to sing-box) -- inserted here,
    // and only when non-empty, matching this module's existing convention
    // of never emitting unused/empty sections (see the `block` outbound
    // above).
    if !rule_set_entries.is_empty() {
        route["rule_set"] = json!(rule_set_entries);
    }

    let mut cfg = json!({
        "log": {
            "level": "info",
            "timestamp": true,
        },
        "inbounds": [inbound],
        "outbounds": outbounds,
        "route": route,
        "experimental": {
            "clash_api": {
                "external_controller": format!("{}:{}", Ipv4Addr::LOCALHOST, clash_api_port),
            },
        },
    });

    if let Some(endpoint) = wireguard_endpoint {
        cfg["endpoints"] = json!([endpoint]);
    }

    cfg
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
            wireguard_private_key: None,
            wireguard_peer_public_key: None,
            wireguard_pre_shared_key: None,
            wireguard_local_address: None,
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
    fn wireguard_endpoint_has_required_fields_with_correct_types() {
        // Note: sing-box 1.13+ has no `wireguard` *outbound* type (removed;
        // see `build_outbound`'s doc comment) -- `build_outbound` returns a
        // WireGuard *endpoint* object for this protocol, which
        // `build_config_with_inbound` places under the config's top-level
        // `endpoints` array rather than `outbounds`. This test exercises
        // `build_outbound` directly, so it checks the endpoint shape.
        let mut server = base_server(Protocol::Wireguard);
        server.wireguard_private_key = Some("gEnPO5uBo93i5evByKmDO5GIqP8CwK201ixEJAN1y1Y=".into());
        server.wireguard_peer_public_key =
            Some("/VydterpEvTguAzYGK2ntJ5JI02e7KqBGsxMC/bqqzQ=".into());
        server.wireguard_local_address = Some("10.0.0.2/32".into());
        let o = build_outbound(&server);

        assert_eq!(o["type"], "wireguard");
        assert_eq!(o["tag"], PROXY_OUTBOUND_TAG);
        assert_eq!(o["private_key"], "gEnPO5uBo93i5evByKmDO5GIqP8CwK201ixEJAN1y1Y=");
        // `address` is array-typed in sing-box even though this app's MVP
        // scope only ever populates one address.
        assert_eq!(o["address"], json!(["10.0.0.2/32"]));
        assert!(o["address"].is_array());
        assert!(o["peers"].is_array());
        assert_eq!(o["peers"][0]["address"], "example.com");
        assert_eq!(o["peers"][0]["port"], 443);
        assert_eq!(o["peers"][0]["public_key"], "/VydterpEvTguAzYGK2ntJ5JI02e7KqBGsxMC/bqqzQ=");
        assert!(o["peers"][0].get("pre_shared_key").is_none());
    }

    #[test]
    fn wireguard_pre_shared_key_emitted_only_when_set() {
        let mut server = base_server(Protocol::Wireguard);
        server.wireguard_pre_shared_key =
            Some("MihvP+gV2j8pb18XF/iI8DrXLj+AfScAscLfmlM2oLU=".into());
        let o = build_outbound(&server);
        assert_eq!(o["peers"][0]["pre_shared_key"], "MihvP+gV2j8pb18XF/iI8DrXLj+AfScAscLfmlM2oLU=");
    }

    #[test]
    fn wireguard_missing_credentials_fall_back_to_empty_strings_not_panic() {
        // Mirrors the other protocols' `unwrap_or_default()` convention --
        // an empty-crypto config is sing-box `check`'s problem to reject, not
        // a Rust-side panic.
        let server = base_server(Protocol::Wireguard);
        let o = build_outbound(&server);
        assert_eq!(o["private_key"], "");
        assert_eq!(o["peers"][0]["public_key"], "");
        assert_eq!(o["address"], json!([""]));
    }

    #[test]
    fn wireguard_never_gets_a_tls_block_even_if_server_has_one_set() {
        // Defense-in-depth: `ServerConfig.tls` should always be `None` for a
        // WireGuard server in practice (the frontend never populates it),
        // but this confirms `build_outbound` itself refuses to attach TLS to
        // a wireguard endpoint even if a caller mistakenly sets one.
        let mut server = base_server(Protocol::Wireguard);
        server.tls = Some(TlsConfig {
            enabled: true,
            server_name: Some("example.com".into()),
            insecure: false,
            reality_public_key: None,
            reality_short_id: None,
        });
        let o = build_outbound(&server);
        assert!(o.get("tls").is_none());
    }

    #[test]
    fn wireguard_server_produces_endpoints_array_not_outbounds_entry() {
        // Confirms `build_config_with_inbound`'s routing of the WireGuard
        // object to the top-level `endpoints` array: the proxy tag still
        // ends up in `outbounds` for every other protocol (see
        // `full_config_has_mixed_inbound_and_final_route`), but for
        // WireGuard it must NOT appear in `outbounds` at all, and
        // `route.final` must still resolve to it via `endpoints`.
        let mut server = base_server(Protocol::Wireguard);
        server.wireguard_private_key = Some("gEnPO5uBo93i5evByKmDO5GIqP8CwK201ixEJAN1y1Y=".into());
        server.wireguard_peer_public_key =
            Some("/VydterpEvTguAzYGK2ntJ5JI02e7KqBGsxMC/bqqzQ=".into());
        server.wireguard_local_address = Some("10.0.0.2/32".into());

        let cfg = build_config(&server, 12345, &[], &HashMap::new(), 9999);

        assert!(cfg["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["type"] != "wireguard"));
        assert_eq!(cfg["outbounds"].as_array().unwrap().len(), 1);
        assert_eq!(cfg["outbounds"][0]["type"], "direct");

        let endpoints = cfg["endpoints"].as_array().expect("endpoints array present");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], "wireguard");
        assert_eq!(endpoints[0]["tag"], PROXY_OUTBOUND_TAG);
        assert_eq!(cfg["route"]["final"], PROXY_OUTBOUND_TAG);
    }

    #[test]
    fn non_wireguard_server_has_no_endpoints_key() {
        let server = base_server(Protocol::Trojan);
        let cfg = build_config(&server, 12345, &[], &HashMap::new(), 9999);
        assert!(cfg.get("endpoints").is_none());
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
        let cfg = build_config(&server, 12345, &[], &HashMap::new(), 9999);
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
        let cfg = build_tun_config(&server, &[], &HashMap::new(), 9999);
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
        let rules = build_route_rules(&[r], &HashMap::new());
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["domain"], json!(["example.com"]));
        assert_eq!(rules[0]["outbound"], DIRECT_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_domain_suffix_maps_to_domain_suffix_field() {
        let r = rule(RuleMatchType::DomainSuffix, &[".cn"], RuleOutbound::Direct);
        let rules = build_route_rules(&[r], &HashMap::new());
        assert_eq!(rules[0]["domain_suffix"], json!([".cn"]));
        assert_eq!(rules[0]["outbound"], DIRECT_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_domain_keyword_maps_to_domain_keyword_field() {
        let r = rule(RuleMatchType::DomainKeyword, &["ads"], RuleOutbound::Block);
        let rules = build_route_rules(&[r], &HashMap::new());
        assert_eq!(rules[0]["domain_keyword"], json!(["ads"]));
        assert_eq!(rules[0]["outbound"], BLOCK_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_ip_cidr_maps_to_ip_cidr_field() {
        let r = rule(RuleMatchType::IpCidr, &["10.0.0.0/8"], RuleOutbound::Block);
        let rules = build_route_rules(&[r], &HashMap::new());
        assert_eq!(rules[0]["ip_cidr"], json!(["10.0.0.0/8"]));
        assert_eq!(rules[0]["outbound"], BLOCK_OUTBOUND_TAG);
    }

    #[test]
    fn route_rule_process_name_maps_to_process_name_field() {
        let r = rule(RuleMatchType::ProcessName, &["chrome.exe"], RuleOutbound::Proxy);
        let rules = build_route_rules(&[r], &HashMap::new());
        assert_eq!(rules[0]["process_name"], json!(["chrome.exe"]));
        assert_eq!(rules[0]["outbound"], PROXY_OUTBOUND_TAG);
    }

    #[test]
    fn disabled_rule_is_excluded() {
        let mut r = rule(RuleMatchType::Domain, &["example.com"], RuleOutbound::Direct);
        r.enabled = false;
        let rules = build_route_rules(&[r], &HashMap::new());
        assert!(rules.is_empty());
    }

    #[test]
    fn empty_values_rule_is_excluded() {
        let r = rule(RuleMatchType::Domain, &[], RuleOutbound::Direct);
        let rules = build_route_rules(&[r], &HashMap::new());
        assert!(rules.is_empty());
    }

    #[test]
    fn block_outbound_absent_when_no_block_rules() {
        let server = base_server(Protocol::Trojan);
        let r = rule(RuleMatchType::Domain, &["example.com"], RuleOutbound::Direct);
        let cfg = build_config(&server, 12345, &[r], &HashMap::new(), 9999);
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
        let cfg = build_config(&server, 12345, &[r], &HashMap::new(), 9999);
        let outbounds = cfg["outbounds"].as_array().unwrap();
        assert_eq!(outbounds.len(), 3);
        assert!(outbounds.iter().any(|o| o["type"] == "block" && o["tag"] == BLOCK_OUTBOUND_TAG));
    }

    #[test]
    fn block_outbound_absent_when_block_rule_disabled() {
        let server = base_server(Protocol::Trojan);
        let mut r = rule(RuleMatchType::IpCidr, &["10.0.0.0/8"], RuleOutbound::Block);
        r.enabled = false;
        let cfg = build_config(&server, 12345, &[r], &HashMap::new(), 9999);
        assert_eq!(cfg["outbounds"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn route_rules_appear_in_generated_config() {
        let server = base_server(Protocol::Trojan);
        let r = rule(RuleMatchType::DomainSuffix, &[".cn"], RuleOutbound::Direct);
        let cfg = build_config(&server, 12345, &[r], &HashMap::new(), 9999);
        assert_eq!(cfg["route"]["rules"].as_array().unwrap().len(), 1);
        assert_eq!(cfg["route"]["rules"][0]["domain_suffix"], json!([".cn"]));
        assert_eq!(cfg["route"]["final"], PROXY_OUTBOUND_TAG);
    }

    #[test]
    fn rule_set_rule_with_known_path_produces_rule_set_and_route_rules_shape() {
        let server = base_server(Protocol::Trojan);
        let mut resource_paths = HashMap::new();
        resource_paths.insert("netflix".to_string(), PathBuf::from("/data/rule-resources/geosite-netflix.srs"));

        let r = rule(RuleMatchType::RuleSet, &["netflix"], RuleOutbound::Proxy);
        let cfg = build_config(&server, 12345, &[r], &resource_paths, 9999);

        let rule_set_entries = cfg["route"]["rule_set"].as_array().expect("rule_set array present");
        assert_eq!(rule_set_entries.len(), 1);
        assert_eq!(rule_set_entries[0]["type"], "local");
        assert_eq!(rule_set_entries[0]["format"], "binary");
        assert_eq!(rule_set_entries[0]["tag"], "ruleset-netflix");
        assert_eq!(rule_set_entries[0]["path"], "/data/rule-resources/geosite-netflix.srs");

        let route_rules = cfg["route"]["rules"].as_array().unwrap();
        assert_eq!(route_rules.len(), 1);
        assert_eq!(route_rules[0]["rule_set"], json!(["ruleset-netflix"]));
        assert_eq!(route_rules[0]["outbound"], PROXY_OUTBOUND_TAG);
        assert!(route_rules[0].get("domain").is_none(), "a RuleSet rule must not also carry a literal match field");
    }

    #[test]
    fn rule_set_rule_with_unknown_id_is_skipped_without_crashing() {
        let server = base_server(Protocol::Trojan);
        // Empty map -- "netflix" was never downloaded (or was deleted).
        let r = rule(RuleMatchType::RuleSet, &["netflix"], RuleOutbound::Proxy);
        let cfg = build_config(&server, 12345, &[r], &HashMap::new(), 9999);

        assert!(cfg["route"].get("rule_set").is_none(), "no rule_set entries should be emitted for an unknown id");
        assert!(cfg["route"]["rules"].as_array().unwrap().is_empty(), "the whole rule should be dropped, not just the unknown id");
    }

    #[test]
    fn rule_set_rule_partially_unknown_ids_only_emits_resolvable_ones() {
        let server = base_server(Protocol::Trojan);
        let mut resource_paths = HashMap::new();
        resource_paths.insert("netflix".to_string(), PathBuf::from("/data/geosite-netflix.srs"));
        // "youtube" deliberately absent from resource_paths.

        let r = rule(RuleMatchType::RuleSet, &["netflix", "youtube"], RuleOutbound::Proxy);
        let cfg = build_config(&server, 12345, &[r], &resource_paths, 9999);

        let rule_set_entries = cfg["route"]["rule_set"].as_array().unwrap();
        assert_eq!(rule_set_entries.len(), 1, "only the resolvable id should get a rule_set entry");
        assert_eq!(rule_set_entries[0]["tag"], "ruleset-netflix");

        let route_rules = cfg["route"]["rules"].as_array().unwrap();
        assert_eq!(route_rules[0]["rule_set"], json!(["ruleset-netflix"]));
    }

    #[test]
    fn mixed_rule_set_and_inline_domain_rules_both_appear() {
        let server = base_server(Protocol::Trojan);
        let mut resource_paths = HashMap::new();
        resource_paths.insert("cn".to_string(), PathBuf::from("/data/geosite-cn.srs"));

        let rule_set_rule = rule(RuleMatchType::RuleSet, &["cn"], RuleOutbound::Direct);
        let domain_rule = rule(RuleMatchType::DomainSuffix, &[".example.com"], RuleOutbound::Proxy);
        let cfg = build_config(&server, 12345, &[rule_set_rule, domain_rule], &resource_paths, 9999);

        let rule_set_entries = cfg["route"]["rule_set"].as_array().unwrap();
        assert_eq!(rule_set_entries.len(), 1);
        assert_eq!(rule_set_entries[0]["tag"], "ruleset-cn");

        let route_rules = cfg["route"]["rules"].as_array().unwrap();
        assert_eq!(route_rules.len(), 2);
        assert_eq!(route_rules[0]["rule_set"], json!(["ruleset-cn"]));
        assert_eq!(route_rules[0]["outbound"], DIRECT_OUTBOUND_TAG);
        assert_eq!(route_rules[1]["domain_suffix"], json!([".example.com"]));
        assert_eq!(route_rules[1]["outbound"], PROXY_OUTBOUND_TAG);
    }

    #[test]
    fn rule_set_referenced_by_two_rules_yields_one_rule_set_entry() {
        let server = base_server(Protocol::Trojan);
        let mut resource_paths = HashMap::new();
        resource_paths.insert("cn".to_string(), PathBuf::from("/data/geosite-cn.srs"));

        let r1 = rule(RuleMatchType::RuleSet, &["cn"], RuleOutbound::Direct);
        let r2 = rule(RuleMatchType::RuleSet, &["cn"], RuleOutbound::Block);
        let cfg = build_config(&server, 12345, &[r1, r2], &resource_paths, 9999);

        let rule_set_entries = cfg["route"]["rule_set"].as_array().unwrap();
        assert_eq!(rule_set_entries.len(), 1, "the same resource id referenced twice should only be emitted once");
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
        let cfg = build_config(&server, 12345, &rules, &HashMap::new(), 9999);

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
        let cfg = build_config(&server, 12345, &[], &HashMap::new(), 19999);

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

    /// Real validation of a `Protocol::Wireguard` server against an actual
    /// sing-box binary's `check -c <file>` subcommand.
    ///
    /// **Important scope note**: sing-box deprecated the `wireguard`
    /// *outbound* type in 1.11.0 and removed it entirely in 1.13.0
    /// (confirmed directly against `.dev-bin/sing-box.exe`, which is
    /// 1.13.19 -- `check` fails with "WireGuard outbound is deprecated ...
    /// and removed in sing-box 1.13.0, use WireGuard endpoint instead" when
    /// fed the old outbound shape). `build_outbound`'s `Wireguard` arm
    /// therefore builds a WireGuard *endpoint* object instead, and
    /// `build_config_with_inbound` places it under the config's top-level
    /// `endpoints` array rather than `outbounds` -- see both functions' doc
    /// comments. This test asserts against that endpoint shape.
    ///
    /// The keys below are real output from `.dev-bin/sing-box.exe generate
    /// wg-keypair` (private key of one keypair as `wireguard_private_key`,
    /// public key of a *different* keypair as `wireguard_peer_public_key`)
    /// and `... generate rand --base64 32` (for `wireguard_pre_shared_key`)
    /// -- `sing-box check` only validates base64 decodability/length, not
    /// real cryptographic validity against a live peer, but a hand-typed
    /// placeholder risks failing that length check in a way that looks like
    /// a bug in this module rather than a bad fixture. Same convention as
    /// the other `real_singbox_*` tests in this file: not run by default
    /// (`#[ignore]`), needs a real binary at
    /// `<workspace root>/.dev-bin/sing-box[.exe]`. Run manually with:
    /// `cargo test -p core-manager --all-targets -- --ignored real_singbox`
    #[test]
    #[ignore = "needs a real sing-box binary at <workspace root>/.dev-bin/"]
    fn real_singbox_check_accepts_wireguard_endpoint() {
        use std::process::Command;

        let binary_name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
        let binary =
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.dev-bin")).join(binary_name);
        assert!(binary.is_file(), "expected a real sing-box binary at {}", binary.display());

        let mut server = base_server(Protocol::Wireguard);
        server.wireguard_private_key = Some("gEnPO5uBo93i5evByKmDO5GIqP8CwK201ixEJAN1y1Y=".into());
        server.wireguard_peer_public_key =
            Some("/VydterpEvTguAzYGK2ntJ5JI02e7KqBGsxMC/bqqzQ=".into());
        server.wireguard_pre_shared_key =
            Some("MihvP+gV2j8pb18XF/iI8DrXLj+AfScAscLfmlM2oLU=".into());
        server.wireguard_local_address = Some("10.0.0.2/32".into());

        let cfg = build_config(&server, 12345, &[], &HashMap::new(), 9999);

        // Sanity-check the endpoint shape before handing this to sing-box,
        // so a failed `check` clearly means "sing-box rejected the shape"
        // rather than "we forgot to build it". Also confirm it did NOT end
        // up in `outbounds` (sing-box 1.13+ would reject that placement).
        assert_eq!(cfg["endpoints"][0]["type"], "wireguard");
        assert!(cfg["endpoints"][0].get("tls").is_none());
        assert!(cfg["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["type"] != "wireguard"));

        let mut path = std::env::temp_dir();
        path.push(format!("ferroflow-config-wireguard-check-{}.json", std::process::id()));
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
            "sing-box check rejected the generated wireguard config.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The most important check in this whole rule-resources feature: a
    /// *real* downloaded `.srs` file, referenced by a `RuleSet` rule, fed to
    /// a real `sing-box check`. Everything else in this file only asserts
    /// against the JSON shape this module produces -- a malformed
    /// `route.rule_set`/`rule_set`-reference shape (wrong field name, wrong
    /// `format`, a `path` sing-box can't open) would only ever surface here.
    ///
    /// Downloads `geosite-netflix.srs` for real via
    /// `rule_resources::download` (same real, small, confirmed-working
    /// upstream file `rule_resources`'s own ignored integration test uses),
    /// to a temp path, references it via a `RuleSet`-type `RoutingRule`, and
    /// asks the real binary to validate the resulting config. Not run by
    /// default (`#[ignore]`) since it needs both a real binary at
    /// `<workspace root>/.dev-bin/sing-box[.exe]` *and* real network access
    /// to `raw.githubusercontent.com` -- run manually with:
    /// `cargo test -p core-manager --all-targets -- --ignored real_singbox`
    #[tokio::test]
    #[ignore = "needs a real sing-box binary at <workspace root>/.dev-bin/ and real network access"]
    async fn real_singbox_check_accepts_config_with_a_real_downloaded_rule_set() {
        use std::process::Command;

        let binary_name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
        let binary =
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.dev-bin")).join(binary_name);
        assert!(binary.is_file(), "expected a real sing-box binary at {}", binary.display());

        let url = rule_resources::resource_url(rule_resources::ResourceCategory::Geosite, "netflix", None);
        let mut srs_path = std::env::temp_dir();
        srs_path.push(format!("ferroflow-core-manager-real-rule-set-{}.srs", std::process::id()));
        let _ = std::fs::remove_file(&srs_path);

        rule_resources::download(&url, &srs_path)
            .await
            .expect("real download of geosite-netflix.srs should succeed");
        assert!(srs_path.is_file(), "downloaded .srs file should exist at {}", srs_path.display());

        let mut resource_paths = HashMap::new();
        resource_paths.insert("netflix".to_string(), srs_path.clone());

        let server = base_server(Protocol::Trojan);
        let r = rule(RuleMatchType::RuleSet, &["netflix"], RuleOutbound::Proxy);
        let cfg = build_config(&server, 12345, &[r], &resource_paths, 9999);

        // Sanity-check the shape before handing this to sing-box, so a
        // failed `check` clearly means "sing-box rejected the shape" rather
        // than "we forgot to build it".
        assert_eq!(cfg["route"]["rule_set"][0]["type"], "local");
        assert_eq!(cfg["route"]["rule_set"][0]["format"], "binary");
        assert_eq!(cfg["route"]["rules"][0]["rule_set"], json!(["ruleset-netflix"]));

        let mut config_path = std::env::temp_dir();
        config_path.push(format!("ferroflow-config-real-rule-set-check-{}.json", std::process::id()));
        std::fs::write(&config_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();

        let output = Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&config_path)
            .output()
            .expect("failed to run sing-box check");

        let _ = std::fs::remove_file(&config_path);
        let _ = std::fs::remove_file(&srs_path);

        assert!(
            output.status.success(),
            "sing-box check rejected the generated config with a real rule_set reference.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
