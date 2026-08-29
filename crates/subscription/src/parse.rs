//! Pure, side-effect-free parsing of subscription bodies and protocol
//! share-links (`vless://`, `trojan://`, `ss://`, `vmess://`) into
//! `shared_types::ServerConfig`. Nothing in this module touches the network
//! or the filesystem -- see `crate::fetch` for that -- which is what makes
//! it fully unit-testable.
//!
//! Field mapping mirrors `core-manager`'s outbound builder
//! (`crates/core-manager/src/config.rs::build_outbound`) exactly: we only
//! extract the fields that builder actually reads (`uuid`, `flow`,
//! `encryption`, `password`, `tls`), and ignore everything else a share-link
//! might carry (transport/`type`, `path`, `headerType`, `alpn`, uTLS
//! fingerprint, ...) since `core-manager` doesn't consume it either.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Deserialize;
use shared_types::{Protocol, ServerConfig, TlsConfig};

/// Substrings that indicate a (decoded) subscription body contains
/// recognizable share-links, used by `decode_subscription_body` to decide
/// whether a successful base64 decode was "real" or just coincidental.
const SCHEME_MARKERS: [&str; 4] = ["vless://", "trojan://", "ss://", "vmess://"];

/// Monotonic counter mixed into generated ids alongside a pid+nanos
/// timestamp -- same lightweight-uniqueness convention as
/// `core_manager::write_temp_config` (pid+nanos is "unique enough" there for
/// one file per call), extended with a counter here since a single
/// `parse_subscription_body` call mints many ids in a tight loop rather than
/// one per call, and two lines parsed within the same nanosecond (plausible
/// on fast machines) would otherwise collide.
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn generate_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sub-{}-{nanos}-{seq}", std::process::id())
}

/// Tries every common base64 alphabet/padding combination against `input`
/// (after stripping embedded whitespace/newlines, which some subscription
/// providers wrap their base64 body with). Returns `None` if none of them
/// produce valid base64.
fn try_base64_decode(input: &str) -> Option<Vec<u8>> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return None;
    }
    STANDARD
        .decode(&cleaned)
        .or_else(|_| STANDARD_NO_PAD.decode(&cleaned))
        .or_else(|_| URL_SAFE.decode(&cleaned))
        .or_else(|_| URL_SAFE_NO_PAD.decode(&cleaned))
        .ok()
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// The fetched subscription body is sometimes the whole thing base64-encoded
/// (the common case for most providers), sometimes plain text with one
/// share-link per line. We try to base64-decode the trimmed body; if that
/// succeeds *and* the result looks like it actually contains share-links
/// (rather than being, say, some unrelated text that happens to be valid
/// base64), we treat it as the decoded form. Otherwise we assume it was
/// already plaintext and return it unchanged.
pub fn decode_subscription_body(body: &str) -> String {
    let trimmed = body.trim();
    if let Some(bytes) = try_base64_decode(trimmed) {
        if let Ok(text) = String::from_utf8(bytes) {
            if SCHEME_MARKERS.iter().any(|marker| text.contains(marker)) {
                return text;
            }
        }
    }
    body.to_string()
}

fn fragment_name(url: &url::Url, fallback_host: &str, fallback_port: u16) -> String {
    url.fragment()
        .map(percent_decode)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{fallback_host}:{fallback_port}"))
}

fn is_truthy_flag(value: &str) -> bool {
    value == "1" || value.eq_ignore_ascii_case("true")
}

/// `vless://<uuid>@<host>:<port>?<query>#<name>`.
fn parse_vless(rest: &str) -> Option<ServerConfig> {
    let url = url::Url::parse(&format!("vless://{rest}")).ok()?;

    let uuid = percent_decode(url.username());
    if uuid.is_empty() {
        return None;
    }
    let host = url.host_str()?.to_string();
    let port = url.port()?;

    let mut flow: Option<String> = None;
    let mut security: Option<String> = None;
    let mut sni: Option<String> = None;
    let mut pbk: Option<String> = None;
    let mut sid: Option<String> = None;
    let mut insecure = false;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "flow" if !value.is_empty() => flow = Some(value.into_owned()),
            "security" => security = Some(value.into_owned()),
            "sni" => sni = Some(value.into_owned()),
            "pbk" => pbk = Some(value.into_owned()),
            "sid" => sid = Some(value.into_owned()),
            "allowInsecure" | "insecure" if is_truthy_flag(&value) => insecure = true,
            _ => {}
        }
    }

    let name = fragment_name(&url, &host, port);

    // `enabled: false` entirely (i.e. `tls: None` on the server) when
    // `security` is absent or `"none"` -- vless share-links with no
    // encryption at all are common for plain internal/relay setups.
    let tls = match security.as_deref() {
        None | Some("none") => None,
        Some(_) => Some(TlsConfig {
            enabled: true,
            server_name: sni,
            insecure,
            reality_public_key: pbk,
            reality_short_id: sid,
        }),
    };

    Some(ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Vless,
        address: host,
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

/// `trojan://<password>@<host>:<port>?<query>#<name>`. Trojan's whole design
/// assumes TLS, so unlike vless this is unconditionally `enabled: true` --
/// trojan share-links don't typically carry a `security` query param the way
/// vless ones do.
fn parse_trojan(rest: &str) -> Option<ServerConfig> {
    let url = url::Url::parse(&format!("trojan://{rest}")).ok()?;

    let password = percent_decode(url.username());
    if password.is_empty() {
        return None;
    }
    let host = url.host_str()?.to_string();
    let port = url.port()?;

    let mut sni: Option<String> = None;
    let mut insecure = false;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "sni" => sni = Some(value.into_owned()),
            "allowInsecure" | "insecure" if is_truthy_flag(&value) => insecure = true,
            _ => {}
        }
    }

    let name = fragment_name(&url, &host, port);

    Some(ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Trojan,
        address: host,
        port,
        uuid: None,
        password: Some(password),
        encryption: None,
        flow: None,
        tls: Some(TlsConfig {
            enabled: true,
            server_name: sni,
            insecure,
            reality_public_key: None,
            reality_short_id: None,
        }),
        wireguard_private_key: None,
        wireguard_peer_public_key: None,
        wireguard_pre_shared_key: None,
        wireguard_local_address: None,
    })
}

/// `ss://<base64(method:password)>@<host>:<port>#<name>` (SIP002, the modern
/// form), with a fallback to a plain (non-base64) `method:password@host:port`
/// userinfo for generators that emit it unencoded. No TLS at this layer --
/// shadowsocks doesn't wrap in TLS, per `core-manager::config`'s own
/// outbound builder.
fn parse_shadowsocks(rest: &str) -> Option<ServerConfig> {
    let url = url::Url::parse(&format!("ss://{rest}")).ok()?;

    let host = url.host_str()?.to_string();
    let port = url.port()?;

    let (method, password) = if let Some(raw_password) = url.password() {
        // Plain, unencoded userinfo: `url` already split it into
        // username/password on the first `:`.
        (percent_decode(url.username()), percent_decode(raw_password))
    } else {
        // SIP002: the whole userinfo is base64(method:password).
        let userinfo = percent_decode(url.username());
        let decoded_bytes = try_base64_decode(&userinfo)?;
        let decoded = String::from_utf8(decoded_bytes).ok()?;
        let mut parts = decoded.splitn(2, ':');
        let method = parts.next()?.to_string();
        let password = parts.next()?.to_string();
        (method, password)
    };

    if method.is_empty() || password.is_empty() {
        return None;
    }

    let name = fragment_name(&url, &host, port);

    Some(ServerConfig {
        id: generate_id(),
        name,
        protocol: Protocol::Shadowsocks,
        address: host,
        port,
        uuid: None,
        password: Some(password),
        encryption: Some(method),
        flow: None,
        tls: None,
        wireguard_private_key: None,
        wireguard_peer_public_key: None,
        wireguard_pre_shared_key: None,
        wireguard_local_address: None,
    })
}

/// Shape of the JSON blob a `vmess://` link base64-encodes. Field names
/// match the de-facto "vmess JSON" format most generators/clients (v2rayN
/// and descendants) use. `scy`/`security` is accepted under either key --
/// generators disagree on which one they emit.
#[derive(Debug, Deserialize)]
struct VmessJson {
    ps: Option<String>,
    add: Option<String>,
    port: Option<serde_json::Value>,
    id: Option<String>,
    /// Not threaded into `ServerConfig` -- `core-manager`'s vmess outbound
    /// builder always emits `alter_id: 0` regardless of what the source
    /// carries (see `config.rs::build_outbound`'s vmess arm). We still
    /// require the key to be present as a sanity check that this is a
    /// well-formed vmess JSON blob and not something else that happens to
    /// parse as JSON.
    aid: Option<serde_json::Value>,
    #[serde(alias = "security")]
    scy: Option<String>,
    tls: Option<String>,
    sni: Option<String>,
    host: Option<String>,
}

fn json_value_to_u16(value: &serde_json::Value) -> Option<u16> {
    match value {
        serde_json::Value::Number(n) => n.as_u64().and_then(|v| u16::try_from(v).ok()),
        serde_json::Value::String(s) => s.trim().parse::<u16>().ok(),
        _ => None,
    }
}

/// `vmess://<base64(json)>` -- unlike the other three schemes this is not a
/// standard URL at all (no `@host:port` authority), so it's decoded and
/// parsed as JSON directly rather than through the `url` crate.
fn parse_vmess(rest: &str) -> Option<ServerConfig> {
    let bytes = try_base64_decode(rest.trim())?;
    let json_str = String::from_utf8(bytes).ok()?;
    let parsed: VmessJson = serde_json::from_str(&json_str).ok()?;

    // Sanity check per the doc comment on `VmessJson::aid` -- confirm the
    // field exists, then discard its value.
    parsed.aid.as_ref()?;

    let uuid = parsed.id.unwrap_or_default();
    if uuid.is_empty() {
        return None;
    }
    let address = parsed.add.unwrap_or_default();
    if address.is_empty() {
        return None;
    }
    let port = parsed.port.as_ref().and_then(json_value_to_u16)?;

    let encryption = parsed.scy.filter(|s| !s.is_empty()).unwrap_or_else(|| "auto".to_string());

    let tls = if parsed.tls.as_deref() == Some("tls") {
        // Some generators put the SNI in the JSON's `host` field (meant for
        // the HTTP Host header of a ws/h2 transport) instead of a dedicated
        // `sni` field -- fall back to that when `sni` itself is absent.
        let sni = parsed
            .sni
            .filter(|s| !s.is_empty())
            .or_else(|| parsed.host.filter(|s| !s.is_empty()));
        Some(TlsConfig {
            enabled: true,
            server_name: sni,
            insecure: false,
            reality_public_key: None,
            reality_short_id: None,
        })
    } else {
        None
    };

    let name = parsed.ps.filter(|s| !s.is_empty()).unwrap_or_else(|| format!("{address}:{port}"));

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

/// Dispatches on the URI's scheme prefix to one of the four protocol
/// parsers. Returns `None` (never `Err`) for anything malformed or
/// unsupported -- a bad line in a subscription should be skippable, not
/// fatal to the whole import.
pub fn parse_uri(uri: &str) -> Option<ServerConfig> {
    let uri = uri.trim();
    if let Some(rest) = uri.strip_prefix("vless://") {
        parse_vless(rest)
    } else if let Some(rest) = uri.strip_prefix("trojan://") {
        parse_trojan(rest)
    } else if let Some(rest) = uri.strip_prefix("ss://") {
        parse_shadowsocks(rest)
    } else if let Some(rest) = uri.strip_prefix("vmess://") {
        parse_vmess(rest)
    } else {
        None
    }
}

/// Decodes `body` (see `decode_subscription_body`), splits it on newlines,
/// and parses each non-empty trimmed line as a share-link. Returns the
/// successfully parsed servers plus a count of lines that were skipped
/// (empty lines are not counted as skipped -- only lines that looked like
/// content but didn't parse).
pub fn parse_subscription_body(body: &str) -> (Vec<ServerConfig>, usize) {
    let decoded = decode_subscription_body(body);
    let mut servers = Vec::new();
    let mut skipped = 0usize;

    for line in decoded.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_uri(line) {
            Some(server) => servers.push(server),
            None => skipped += 1,
        }
    }

    (servers, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- vless -------------------------------------------------------

    #[test]
    fn vless_reality_parses_all_fields() {
        let uri = "vless://b831381d-6324-4d53-ad4f-8cda48b30811@server.example.com:443?\
                   encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.microsoft.com\
                   &fp=chrome&pbk=zGJ8bLKEwSVsWtqdCFHYFsWkYUAqXLXHo9EorGiPKk8&sid=6ba85179e30d4fc2\
                   &type=tcp&headerType=none#My%20Reality%20Server";
        let server = parse_uri(uri).expect("should parse");
        assert_eq!(server.protocol, Protocol::Vless);
        assert_eq!(server.address, "server.example.com");
        assert_eq!(server.port, 443);
        assert_eq!(server.uuid.as_deref(), Some("b831381d-6324-4d53-ad4f-8cda48b30811"));
        assert_eq!(server.flow.as_deref(), Some("xtls-rprx-vision"));
        assert_eq!(server.name, "My Reality Server");

        let tls = server.tls.expect("tls should be set for security=reality");
        assert!(tls.enabled);
        assert_eq!(tls.server_name.as_deref(), Some("www.microsoft.com"));
        assert_eq!(tls.reality_public_key.as_deref(), Some("zGJ8bLKEwSVsWtqdCFHYFsWkYUAqXLXHo9EorGiPKk8"));
        assert_eq!(tls.reality_short_id.as_deref(), Some("6ba85179e30d4fc2"));
        assert!(!tls.insecure);
    }

    #[test]
    fn vless_plain_tls_with_insecure_flag() {
        let uri = "vless://11111111-2222-3333-4444-555555555555@vless.example.com:8443?\
                   security=tls&sni=vless.example.com&allowInsecure=1&type=ws#Vless%20TLS";
        let server = parse_uri(uri).expect("should parse");
        let tls = server.tls.expect("tls should be set");
        assert!(tls.enabled);
        assert!(tls.insecure);
        assert!(tls.reality_public_key.is_none());
        assert_eq!(server.name, "Vless TLS");
    }

    #[test]
    fn vless_security_none_has_no_tls() {
        let uri = "vless://11111111-2222-3333-4444-555555555555@plain.example.com:80?\
                   type=tcp&security=none#Plain%20Vless";
        let server = parse_uri(uri).expect("should parse");
        assert!(server.tls.is_none());
    }

    #[test]
    fn vless_missing_security_has_no_tls() {
        let uri = "vless://11111111-2222-3333-4444-555555555555@plain.example.com:80";
        let server = parse_uri(uri).expect("should parse");
        assert!(server.tls.is_none());
    }

    #[test]
    fn vless_falls_back_to_host_port_name_when_no_fragment() {
        let uri = "vless://11111111-2222-3333-4444-555555555555@no-name.example.com:1234";
        let server = parse_uri(uri).expect("should parse");
        assert_eq!(server.name, "no-name.example.com:1234");
    }

    #[test]
    fn vless_rejects_empty_uuid() {
        let uri = "vless://@no-uuid.example.com:443?security=none";
        assert!(parse_uri(uri).is_none());
    }

    #[test]
    fn vless_rejects_missing_port() {
        let uri = "vless://11111111-2222-3333-4444-555555555555@no-port.example.com?security=none";
        assert!(parse_uri(uri).is_none());
    }

    // ---- trojan --------------------------------------------------------

    #[test]
    fn trojan_parses_password_and_sni() {
        let uri = "trojan://p%40ssword@trojan.example.com:443?sni=trojan.example.com&allowInsecure=1#Trojan%20Server";
        let server = parse_uri(uri).expect("should parse");
        assert_eq!(server.protocol, Protocol::Trojan);
        assert_eq!(server.password.as_deref(), Some("p@ssword"));
        assert_eq!(server.address, "trojan.example.com");
        assert_eq!(server.port, 443);
        assert_eq!(server.name, "Trojan Server");

        let tls = server.tls.expect("trojan is always tls");
        assert!(tls.enabled);
        assert_eq!(tls.server_name.as_deref(), Some("trojan.example.com"));
        assert!(tls.insecure);
    }

    #[test]
    fn trojan_is_always_tls_even_without_query_params() {
        let uri = "trojan://simplepassword@trojan2.example.com:443";
        let server = parse_uri(uri).expect("should parse");
        let tls = server.tls.expect("trojan is always tls");
        assert!(tls.enabled);
        assert!(tls.server_name.is_none());
        assert!(!tls.insecure);
    }

    #[test]
    fn trojan_rejects_empty_password() {
        let uri = "trojan://@no-password.example.com:443";
        assert!(parse_uri(uri).is_none());
    }

    // ---- shadowsocks -----------------------------------------------------

    #[test]
    fn shadowsocks_sip002_base64_userinfo() {
        let userinfo = STANDARD.encode("aes-256-gcm:S3cr3tPass");
        let uri = format!("ss://{userinfo}@ss.example.com:8388#SS%20SIP002");
        let server = parse_uri(&uri).expect("should parse");
        assert_eq!(server.protocol, Protocol::Shadowsocks);
        assert_eq!(server.encryption.as_deref(), Some("aes-256-gcm"));
        assert_eq!(server.password.as_deref(), Some("S3cr3tPass"));
        assert_eq!(server.address, "ss.example.com");
        assert_eq!(server.port, 8388);
        assert_eq!(server.name, "SS SIP002");
        assert!(server.tls.is_none());
    }

    #[test]
    fn shadowsocks_plain_unencoded_userinfo() {
        let uri = "ss://chacha20-ietf-poly1305:anotherpass@ss2.example.com:8388#SS%20Plain";
        let server = parse_uri(uri).expect("should parse");
        assert_eq!(server.encryption.as_deref(), Some("chacha20-ietf-poly1305"));
        assert_eq!(server.password.as_deref(), Some("anotherpass"));
        assert_eq!(server.name, "SS Plain");
        assert!(server.tls.is_none());
    }

    #[test]
    fn shadowsocks_falls_back_to_host_port_name() {
        let userinfo = STANDARD.encode("aes-256-gcm:S3cr3tPass");
        let uri = format!("ss://{userinfo}@ss3.example.com:8388");
        let server = parse_uri(&uri).expect("should parse");
        assert_eq!(server.name, "ss3.example.com:8388");
    }

    #[test]
    fn shadowsocks_rejects_garbage_userinfo() {
        // `!` is not part of any base64 alphabet we try, and there's no `:`
        // for the plain fallback either, so this should fail to decode.
        let uri = "ss://invalid-user!info@ss4.example.com:8388";
        assert!(parse_uri(uri).is_none());
    }

    // ---- vmess -------------------------------------------------------

    fn encode_vmess_json(json: &str) -> String {
        format!("vmess://{}", STANDARD.encode(json))
    }

    #[test]
    fn vmess_with_tls_and_explicit_sni() {
        let json = r#"{"v":"2","ps":"Vmess TLS Node","add":"vmess.example.com","port":"443",
                       "id":"c1a2b3c4-d5e6-f708-1920-a1b2c3d4e5f6","aid":"0","net":"ws",
                       "type":"none","host":"host-header.example.com","path":"/ws",
                       "tls":"tls","sni":"cdn.example.com"}"#;
        let uri = encode_vmess_json(json);
        let server = parse_uri(&uri).expect("should parse");
        assert_eq!(server.protocol, Protocol::Vmess);
        assert_eq!(server.name, "Vmess TLS Node");
        assert_eq!(server.address, "vmess.example.com");
        assert_eq!(server.port, 443);
        assert_eq!(server.uuid.as_deref(), Some("c1a2b3c4-d5e6-f708-1920-a1b2c3d4e5f6"));
        assert_eq!(server.encryption.as_deref(), Some("auto"));

        let tls = server.tls.expect("tls: \"tls\" should enable tls");
        assert!(tls.enabled);
        // Explicit `sni` wins over the JSON's `host` field.
        assert_eq!(tls.server_name.as_deref(), Some("cdn.example.com"));
    }

    #[test]
    fn vmess_sni_falls_back_to_host_field() {
        let json = r#"{"ps":"Vmess Fallback SNI","add":"vmess2.example.com","port":443,
                       "id":"11111111-2222-3333-4444-555555555555","aid":0,
                       "host":"sni-from-host.example.com","tls":"tls"}"#;
        let uri = encode_vmess_json(json);
        let server = parse_uri(&uri).expect("should parse");
        let tls = server.tls.expect("tls should be set");
        assert_eq!(tls.server_name.as_deref(), Some("sni-from-host.example.com"));
    }

    #[test]
    fn vmess_without_tls_has_no_tls_block() {
        let json = r#"{"ps":"Vmess Plain","add":"vmess3.example.com","port":80,
                       "id":"11111111-2222-3333-4444-555555555555","aid":0}"#;
        let uri = encode_vmess_json(json);
        let server = parse_uri(&uri).expect("should parse");
        assert!(server.tls.is_none());
        assert_eq!(server.encryption.as_deref(), Some("auto"));
    }

    #[test]
    fn vmess_accepts_security_key_as_alias_for_scy() {
        let json = r#"{"ps":"Vmess Security Alias","add":"vmess4.example.com","port":443,
                       "id":"11111111-2222-3333-4444-555555555555","aid":0,"security":"zero"}"#;
        let uri = encode_vmess_json(json);
        let server = parse_uri(&uri).expect("should parse");
        assert_eq!(server.encryption.as_deref(), Some("zero"));
    }

    #[test]
    fn vmess_falls_back_to_host_port_name_without_ps() {
        let json = r#"{"add":"vmess5.example.com","port":443,
                       "id":"11111111-2222-3333-4444-555555555555","aid":0}"#;
        let uri = encode_vmess_json(json);
        let server = parse_uri(&uri).expect("should parse");
        assert_eq!(server.name, "vmess5.example.com:443");
    }

    #[test]
    fn vmess_rejects_missing_aid() {
        let json = r#"{"ps":"No Aid","add":"vmess6.example.com","port":443,
                       "id":"11111111-2222-3333-4444-555555555555"}"#;
        let uri = encode_vmess_json(json);
        assert!(parse_uri(&uri).is_none());
    }

    #[test]
    fn vmess_rejects_missing_uuid() {
        let json = r#"{"ps":"No Id","add":"vmess7.example.com","port":443,"aid":0}"#;
        let uri = encode_vmess_json(json);
        assert!(parse_uri(&uri).is_none());
    }

    #[test]
    fn vmess_rejects_invalid_base64() {
        assert!(parse_uri("vmess://not-valid-base64-json!!!").is_none());
    }

    // ---- dispatch / unsupported ----------------------------------------

    #[test]
    fn parse_uri_returns_none_for_unsupported_scheme() {
        assert!(parse_uri("hysteria2://uuid@host:443").is_none());
        assert!(parse_uri("not a uri at all").is_none());
        assert!(parse_uri("").is_none());
    }

    // ---- decode_subscription_body --------------------------------------

    #[test]
    fn decode_subscription_body_decodes_base64_wrapped_body() {
        let plain = "vless://11111111-2222-3333-4444-555555555555@a.example.com:443?security=none#A\n\
                     trojan://pw@b.example.com:443#B\n";
        let wrapped = STANDARD.encode(plain);
        assert_eq!(decode_subscription_body(&wrapped), plain);
    }

    #[test]
    fn decode_subscription_body_leaves_plaintext_unchanged() {
        let plain = "vless://11111111-2222-3333-4444-555555555555@a.example.com:443?security=none#A";
        assert_eq!(decode_subscription_body(plain), plain);
    }

    #[test]
    fn decode_subscription_body_does_not_misfire_on_unrelated_base64() {
        // Valid base64, decodes to valid utf8, but doesn't contain any of
        // our scheme markers -- should be treated as "not actually encoded"
        // and returned unchanged (as if it were plaintext, even though as
        // plaintext it's nonsense -- that's fine, it'll just skip when
        // parsed line by line).
        let unrelated = STANDARD.encode("just some unrelated base64 content");
        assert_eq!(decode_subscription_body(&unrelated), unrelated);
    }

    // ---- parse_subscription_body ----------------------------------------

    #[test]
    fn parse_subscription_body_mixed_valid_and_garbage_lines() {
        let userinfo = STANDARD.encode("aes-256-gcm:S3cr3tPass");
        let body = format!(
            "vless://11111111-2222-3333-4444-555555555555@a.example.com:443?security=none#A\n\
             \n\
             this is not a uri\n\
             trojan://pw@b.example.com:443#B\n\
             ss://{userinfo}@c.example.com:8388#C\n\
             hysteria2://unsupported@d.example.com:443\n"
        );
        let (servers, skipped) = parse_subscription_body(&body);
        assert_eq!(servers.len(), 3);
        assert_eq!(skipped, 2);
        assert_eq!(servers[0].protocol, Protocol::Vless);
        assert_eq!(servers[1].protocol, Protocol::Trojan);
        assert_eq!(servers[2].protocol, Protocol::Shadowsocks);
    }

    #[test]
    fn parse_subscription_body_handles_base64_wrapped_input() {
        let plain = "trojan://pw@a.example.com:443#A\ntrojan://pw2@b.example.com:443#B\n";
        let wrapped = STANDARD.encode(plain);
        let (servers, skipped) = parse_subscription_body(&wrapped);
        assert_eq!(servers.len(), 2);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn parse_subscription_body_empty_input_yields_nothing() {
        let (servers, skipped) = parse_subscription_body("");
        assert!(servers.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn generated_ids_are_unique_within_a_batch() {
        let body = "trojan://pw@a.example.com:443#A\ntrojan://pw@a.example.com:443#A\ntrojan://pw@a.example.com:443#A\n";
        let (servers, _) = parse_subscription_body(body);
        let ids: std::collections::HashSet<_> = servers.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), servers.len());
    }
}
