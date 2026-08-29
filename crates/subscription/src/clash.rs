//! Clash-style YAML config import: extracts the top-level `proxies:` list
//! from a Clash config and converts each entry into a `shared_types::ServerConfig`,
//! covering the same four protocols `subscription::parse` and this app's
//! `core-manager` outbound builder support -- `vless`, `trojan`, `ss`
//! (shadowsocks), and `vmess`. Any other `type` (e.g. `hysteria2`, `ssr`,
//! `snell`) is skipped, same as an unsupported scheme in a share-link.
//!
//! Deliberately lenient: real-world Clash configs vary in field naming
//! across generators (`reality-opts` vs `reality_opts`, `skip-cert-verify`
//! vs `insecure`, ...), so this parses via `serde_yaml::Value` and pulls
//! fields out by hand rather than deserializing into one strict struct --
//! one malformed/unrecognized entry is skipped (and counted), never fatal to
//! the whole batch, mirroring `parse_subscription_body`'s per-line policy.
//!
//! Field mapping mirrors `parse.rs`'s doc comment: only the fields
//! `core-manager`'s outbound builder actually reads (`uuid`, `flow`,
//! `encryption`, `password`, TLS/Reality settings) are extracted --
//! transport options (`network`, `ws-opts`, `grpc-opts`, ...) are parsed by
//! no one and ignored here either.

use serde_yaml::{Mapping, Value};
use shared_types::{Protocol, ServerConfig, TlsConfig};

use crate::parse::generate_id;

fn get_str(map: &Mapping, key: &str) -> Option<String> {
    match map.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn get_bool(map: &Mapping, key: &str) -> bool {
    match map.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn get_port(map: &Mapping, key: &str) -> Option<u16> {
    match map.get(key)? {
        Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        Value::String(s) => s.trim().parse::<u16>().ok(),
        _ => None,
    }
}

fn get_nested<'a>(map: &'a Mapping, key: &str) -> Option<&'a Mapping> {
    map.get(key).and_then(Value::as_mapping)
}

/// Some generators use hyphenated keys (`reality-opts`), others underscored
/// (`reality_opts`) -- try both rather than picking one.
fn get_str_either(map: &Mapping, key_a: &str, key_b: &str) -> Option<String> {
    get_str(map, key_a).or_else(|| get_str(map, key_b))
}

fn convert_vless(proxy: &Mapping, name: String) -> Option<ServerConfig> {
    let address = get_str(proxy, "server")?;
    let port = get_port(proxy, "port")?;
    let uuid = get_str(proxy, "uuid")?;
    let flow = get_str(proxy, "flow").filter(|s| !s.is_empty());

    let tls_enabled = get_bool(proxy, "tls");
    let tls = if tls_enabled {
        let reality = get_nested(proxy, "reality-opts").or_else(|| get_nested(proxy, "reality_opts"));
        let (reality_public_key, reality_short_id) = match reality {
            Some(opts) => (
                get_str_either(opts, "public-key", "public_key"),
                get_str_either(opts, "short-id", "short_id"),
            ),
            None => (None, None),
        };
        Some(TlsConfig {
            enabled: true,
            server_name: get_str(proxy, "servername").or_else(|| get_str(proxy, "sni")),
            insecure: get_bool(proxy, "skip-cert-verify") || get_bool(proxy, "insecure"),
            reality_public_key,
            reality_short_id,
        })
    } else {
        None
    };

    Some(ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Vless,
        address,
        port,
        uuid: Some(uuid),
        password: None,
        encryption: None,
        flow,
        tls,
        wireguard_private_key: None,
        wireguard_peer_public_key: None,
        wireguard_pre_shared_key: None,
        wireguard_local_address: None,
    })
}

fn convert_trojan(proxy: &Mapping, name: String) -> Option<ServerConfig> {
    let address = get_str(proxy, "server")?;
    let port = get_port(proxy, "port")?;
    let password = get_str(proxy, "password")?;

    Some(ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Trojan,
        address,
        port,
        uuid: None,
        password: Some(password),
        encryption: None,
        flow: None,
        // Trojan is unconditionally TLS, same as the share-link parser.
        tls: Some(TlsConfig {
            enabled: true,
            server_name: get_str(proxy, "sni").or_else(|| get_str(proxy, "servername")),
            insecure: get_bool(proxy, "skip-cert-verify") || get_bool(proxy, "insecure"),
            reality_public_key: None,
            reality_short_id: None,
        }),
        wireguard_private_key: None,
        wireguard_peer_public_key: None,
        wireguard_pre_shared_key: None,
        wireguard_local_address: None,
    })
}

fn convert_shadowsocks(proxy: &Mapping, name: String) -> Option<ServerConfig> {
    let address = get_str(proxy, "server")?;
    let port = get_port(proxy, "port")?;
    let cipher = get_str(proxy, "cipher")?;
    let password = get_str(proxy, "password")?;

    Some(ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Shadowsocks,
        address,
        port,
        uuid: None,
        password: Some(password),
        encryption: Some(cipher),
        flow: None,
        tls: None,
        wireguard_private_key: None,
        wireguard_peer_public_key: None,
        wireguard_pre_shared_key: None,
        wireguard_local_address: None,
    })
}

fn convert_vmess(proxy: &Mapping, name: String) -> Option<ServerConfig> {
    let address = get_str(proxy, "server")?;
    let port = get_port(proxy, "port")?;
    let uuid = get_str(proxy, "uuid")?;
    let encryption = get_str(proxy, "cipher").filter(|s| !s.is_empty()).unwrap_or_else(|| "auto".to_string());

    let tls = if get_bool(proxy, "tls") {
        Some(TlsConfig {
            enabled: true,
            server_name: get_str(proxy, "servername").or_else(|| get_str(proxy, "sni")),
            insecure: get_bool(proxy, "skip-cert-verify") || get_bool(proxy, "insecure"),
            reality_public_key: None,
            reality_short_id: None,
        })
    } else {
        None
    };

    Some(ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Vmess,
        address,
        port,
        uuid: Some(uuid),
        password: None,
        encryption: Some(encryption),
        flow: None,
        tls,
        wireguard_private_key: None,
        wireguard_peer_public_key: None,
        wireguard_pre_shared_key: None,
        wireguard_local_address: None,
    })
}

fn convert_proxy(value: &Value) -> Option<ServerConfig> {
    let proxy = value.as_mapping()?;
    let proxy_type = get_str(proxy, "type")?;
    let name = get_str(proxy, "name").filter(|s| !s.is_empty()).unwrap_or_else(|| proxy_type.clone());

    match proxy_type.as_str() {
        "vless" => convert_vless(proxy, name),
        "trojan" => convert_trojan(proxy, name),
        "ss" => convert_shadowsocks(proxy, name),
        "vmess" => convert_vmess(proxy, name),
        _ => None,
    }
}

/// Parses `body` as a Clash-style YAML config and converts its `proxies:`
/// list into `ServerConfig`s. Returns the successfully converted servers
/// plus a count of entries that were skipped -- an unparseable document (not
/// valid YAML at all, or no top-level `proxies` sequence) yields `(vec![], 0)`
/// rather than an error, matching `parse_subscription_body`'s "never fatal,
/// just skip" policy; the caller (`commands::subscription`) is the one that
/// turns an empty result into a user-facing `subscription_empty` error.
pub fn parse_clash_yaml(body: &str) -> (Vec<ServerConfig>, usize) {
    let Ok(doc) = serde_yaml::from_str::<Value>(body) else {
        return (Vec::new(), 0);
    };
    let Some(proxies) = doc.as_mapping().and_then(|m| m.get("proxies")).and_then(Value::as_sequence) else {
        return (Vec::new(), 0);
    };

    let mut servers = Vec::new();
    let mut skipped = 0usize;
    for entry in proxies {
        match convert_proxy(entry) {
            Some(server) => servers.push(server),
            None => skipped += 1,
        }
    }

    (servers, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
proxies:
  - name: "example"
    type: vless
    server: 1.2.3.4
    port: 443
    uuid: "11111111-2222-3333-4444-555555555555"
    tls: true
    servername: example.com
    reality-opts:
      public-key: "pubkey123"
      short-id: "shortid456"
  - name: "example2"
    type: trojan
    server: 1.2.3.4
    port: 443
    password: "trojanpass"
    sni: example.com
  - name: "example3"
    type: ss
    server: 1.2.3.4
    port: 8388
    cipher: aes-256-gcm
    password: "sspass"
  - name: "example4"
    type: vmess
    server: 1.2.3.4
    port: 443
    uuid: "66666666-7777-8888-9999-000000000000"
    alterId: 0
    cipher: auto
    tls: true
  - name: "unsupported"
    type: hysteria2
    server: 1.2.3.4
    port: 443
"#;

    #[test]
    fn parses_all_four_supported_types_and_skips_the_fifth() {
        let (servers, skipped) = parse_clash_yaml(SAMPLE);
        assert_eq!(servers.len(), 4);
        assert_eq!(skipped, 1);

        assert_eq!(servers[0].protocol, Protocol::Vless);
        assert_eq!(servers[0].name, "example");
        assert_eq!(servers[0].address, "1.2.3.4");
        assert_eq!(servers[0].port, 443);
        assert_eq!(servers[0].uuid.as_deref(), Some("11111111-2222-3333-4444-555555555555"));
        let tls = servers[0].tls.as_ref().expect("tls should be set");
        assert!(tls.enabled);
        assert_eq!(tls.server_name.as_deref(), Some("example.com"));
        assert_eq!(tls.reality_public_key.as_deref(), Some("pubkey123"));
        assert_eq!(tls.reality_short_id.as_deref(), Some("shortid456"));

        assert_eq!(servers[1].protocol, Protocol::Trojan);
        assert_eq!(servers[1].password.as_deref(), Some("trojanpass"));
        assert!(servers[1].tls.as_ref().unwrap().enabled);

        assert_eq!(servers[2].protocol, Protocol::Shadowsocks);
        assert_eq!(servers[2].encryption.as_deref(), Some("aes-256-gcm"));
        assert_eq!(servers[2].password.as_deref(), Some("sspass"));
        assert!(servers[2].tls.is_none());

        assert_eq!(servers[3].protocol, Protocol::Vmess);
        assert_eq!(servers[3].uuid.as_deref(), Some("66666666-7777-8888-9999-000000000000"));
        assert_eq!(servers[3].encryption.as_deref(), Some("auto"));
        assert!(servers[3].tls.as_ref().unwrap().enabled);
    }

    #[test]
    fn missing_required_field_skips_entry_not_whole_batch() {
        let body = r#"
proxies:
  - name: "no-uuid"
    type: vless
    server: 1.2.3.4
    port: 443
  - name: "good"
    type: trojan
    server: 1.2.3.4
    port: 443
    password: "pw"
"#;
        let (servers, skipped) = parse_clash_yaml(body);
        assert_eq!(servers.len(), 1);
        assert_eq!(skipped, 1);
        assert_eq!(servers[0].protocol, Protocol::Trojan);
    }

    #[test]
    fn no_proxies_key_yields_nothing() {
        let (servers, skipped) = parse_clash_yaml("some: other\nyaml: true\n");
        assert!(servers.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn invalid_yaml_yields_nothing_not_a_panic() {
        let (servers, skipped) = parse_clash_yaml("not: valid: yaml: [");
        assert!(servers.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn underscored_reality_opts_key_is_also_accepted() {
        let body = r#"
proxies:
  - name: "underscored"
    type: vless
    server: 1.2.3.4
    port: 443
    uuid: "11111111-2222-3333-4444-555555555555"
    tls: true
    reality_opts:
      public_key: "pk"
      short_id: "sid"
"#;
        let (servers, _) = parse_clash_yaml(body);
        assert_eq!(servers.len(), 1);
        let tls = servers[0].tls.as_ref().unwrap();
        assert_eq!(tls.reality_public_key.as_deref(), Some("pk"));
        assert_eq!(tls.reality_short_id.as_deref(), Some("sid"));
    }

    #[test]
    fn generated_ids_are_unique() {
        let (servers, _) = parse_clash_yaml(SAMPLE);
        let ids: std::collections::HashSet<_> = servers.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), servers.len());
    }
}
