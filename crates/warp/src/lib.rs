//! One-click Cloudflare WARP registration: turns a free, anonymous WARP
//! device registration into a WireGuard [`WarpRegistration`] the caller can
//! map directly onto `shared_types::ServerConfig` (see
//! `src-tauri/src/commands/warp.rs`).
//!
//! This talks to Cloudflare's real, public `api.cloudflareclient.com`
//! registration API -- the same unauthenticated, self-service endpoint the
//! well-known open-source `wgcf` project (and the official WARP mobile
//! apps) use to mint anonymous WireGuard identities. There is no
//! account/credential requirement: any caller can `POST` a freshly generated
//! X25519 public key and get back a working WireGuard peer config. This is
//! not reverse-engineering an undocumented private API -- it's the same
//! flow `wgcf` has relied on for years.
//!
//! Two deliberate fixed-value choices, both confirmed against a real,
//! successful registration during development (see this crate's ignored
//! live integration test):
//! - The response's `config.peers[0].endpoint.v4` carries a placeholder
//!   `:0` port -- we strip it and pair the bare IP with a **fixed** port of
//!   [`WARP_ENDPOINT_PORT`] (2408, WARP's well-known primary UDP port and
//!   `wgcf`'s own default), rather than trusting the placeholder or
//!   resolving `endpoint.host` at runtime.
//! - The response's `policy.tunnel_protocol` field (not modeled here) may
//!   say `"masque"`, Cloudflare's newer default hint -- irrelevant to this
//!   crate, since sing-box has no MASQUE support and the classic WireGuard
//!   fields in `config.peers[0]`/`config.interface` remain fully valid
//!   regardless of that hint (again, exactly what `wgcf` relies on).

use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::header::{AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};

const REG_BASE_URL: &str = "https://api.cloudflareclient.com/v0a2158/reg";
const USER_AGENT_VALUE: &str = "okhttp/3.12.1";
const CF_CLIENT_VERSION: &str = "a-6.30-2158";
/// Accepted by Cloudflare's registration API as a valid (past) ToS
/// acceptance timestamp -- confirmed against a real registration. A fixed
/// literal rather than "now" so `register()` doesn't need a
/// date/time-formatting dependency for one field Cloudflare doesn't appear
/// to validate against the current date anyway.
const TOS_TIMESTAMP: &str = "2024-01-01T00:00:00.000Z";
/// WARP's well-known primary UDP port (`config.peers[0].endpoint.ports[0]`
/// in a real response, and `wgcf`'s own default) -- used as a fixed value
/// rather than trusting the placeholder `:0` port embedded in
/// `endpoint.v4`. See this module's doc comment.
pub const WARP_ENDPOINT_PORT: u16 = 2408;

/// This is a real external network call to Cloudflare's API, not a loopback
/// call to a process we already know is up -- generous but bounded.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Failure modes for [`register`]/[`deregister`], each carrying enough
/// context to be surfaced directly to a user as a toast/error message.
#[derive(Debug, Error)]
pub enum WarpError {
    #[error("could not build an HTTP client: {0}")]
    ClientBuild(reqwest::Error),
    #[error("request to '{url}' failed: {source}")]
    Request { url: String, source: reqwest::Error },
    #[error("failed to parse response from '{url}': {source}")]
    Deserialize { url: String, source: reqwest::Error },
    /// A non-2xx response from Cloudflare's API. `message` is the API's own
    /// documented `{"success":false,"errors":[{"code":...,"message":"..."}]}`
    /// error text when the body parsed as that shape, or the raw response
    /// body otherwise -- never just a bare status code with no detail.
    #[error("Cloudflare WARP API returned HTTP {status}: {message}")]
    Api { status: u16, message: String },
    /// The response was valid JSON and a 2xx status, but was missing a field
    /// this crate actually needs (e.g. no peers in `config.peers`).
    #[error("Cloudflare WARP registration response is missing expected field: {0}")]
    MissingField(&'static str),
}

/// A completed, ready-to-use anonymous WARP device registration, already
/// shaped for a straight field-by-field mapping onto
/// `shared_types::ServerConfig`'s `wireguard_*` fields (see
/// `src-tauri/src/commands/warp.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarpRegistration {
    /// Cloudflare's device id for this registration (`response.id`) --
    /// needed later to call [`deregister`].
    pub device_id: String,
    /// Cloudflare's bearer token for this device (`response.token`) --
    /// needed later to call [`deregister`].
    pub token: String,
    /// Base64-encoded 32-byte X25519 private key, generated locally. Never
    /// sent anywhere but Cloudflare's own registration call (only the
    /// corresponding public key is transmitted).
    pub private_key: String,
    /// Base64-encoded 32-byte X25519 public key of Cloudflare's WireGuard
    /// peer, from `response.config.peers[0].public_key`.
    pub peer_public_key: String,
    /// Bare IP address (no port), from `response.config.peers[0].endpoint.v4`
    /// with the placeholder `:0` suffix stripped.
    pub endpoint_address: String,
    /// Always [`WARP_ENDPOINT_PORT`] -- see this module's doc comment for
    /// why the response's own placeholder port isn't used.
    pub endpoint_port: u16,
    /// This client's local tunnel address in CIDR form, e.g.
    /// `"172.16.0.2/32"` -- `response.config.interface.addresses.v4`
    /// combined with a `/32` suffix.
    pub local_address_v4: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct RegisterRequest {
    key: String,
    install_id: String,
    fcm_token: String,
    tos: String,
    #[serde(rename = "type")]
    device_type: String,
    locale: String,
}

fn build_register_request(public_key_b64: &str) -> RegisterRequest {
    RegisterRequest {
        key: public_key_b64.to_string(),
        install_id: String::new(),
        fcm_token: String::new(),
        tos: TOS_TIMESTAMP.to_string(),
        device_type: "Android".to_string(),
        locale: "en_US".to_string(),
    }
}

/// Only the fields this crate actually reads are modeled -- a real response
/// carries several more (`account`, `policy`, `warp_enabled`, `waitlist_enabled`,
/// ...) that nothing here needs.
#[derive(Debug, Deserialize)]
struct RegisterResponse {
    id: String,
    token: String,
    config: RegisterConfig,
}

#[derive(Debug, Deserialize)]
struct RegisterConfig {
    peers: Vec<RegisterPeer>,
    interface: RegisterInterface,
}

#[derive(Debug, Deserialize)]
struct RegisterPeer {
    public_key: String,
    endpoint: RegisterEndpoint,
}

#[derive(Debug, Deserialize)]
struct RegisterEndpoint {
    v4: String,
}

#[derive(Debug, Deserialize)]
struct RegisterInterface {
    addresses: RegisterAddresses,
}

#[derive(Debug, Deserialize)]
struct RegisterAddresses {
    v4: String,
}

/// Cloudflare's documented error shape:
/// `{"success":false,"errors":[{"code":...,"message":"..."}]}`.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    errors: Vec<ApiErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    code: i64,
    message: String,
}

/// Strips a trailing `:<port>` from an IPv4 `host:port` string (used on
/// `endpoint.v4`, which carries a placeholder `:0`). IPv4 addresses never
/// contain a colon themselves, so splitting on the first (and only) `:` is
/// unambiguous. Returns the input unchanged if there's no colon at all.
fn strip_port(host_port: &str) -> String {
    host_port.split(':').next().unwrap_or(host_port).to_string()
}

/// Builds a [`WarpError::Api`] from a non-2xx response body: tries to parse
/// it as Cloudflare's documented `{"success":false,"errors":[...]}` shape
/// first, falling back to the raw body text (or a placeholder when the body
/// is empty) so the caller never sees a bare, contextless status code.
fn parse_error_response(status: reqwest::StatusCode, body: &str) -> WarpError {
    let message = serde_json::from_str::<ApiErrorResponse>(body)
        .ok()
        .map(|parsed| {
            parsed
                .errors
                .iter()
                .map(|e| format!("{} ({})", e.message, e.code))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                "no error details provided".to_string()
            } else {
                trimmed.to_string()
            }
        });
    WarpError::Api { status: status.as_u16(), message }
}

/// Pure transform from a parsed [`RegisterResponse`] (plus the private key
/// we generated locally, which never appears in the response) into a
/// [`WarpRegistration`]. Kept separate from `register()` so the
/// field-mapping/port-stripping logic is unit-testable against a
/// hand-constructed fixture without a real network call.
fn build_registration(
    response: RegisterResponse,
    private_key_b64: String,
) -> Result<WarpRegistration, WarpError> {
    let peer = response.config.peers.into_iter().next().ok_or(WarpError::MissingField("config.peers[0]"))?;
    let endpoint_address = strip_port(&peer.endpoint.v4);
    if endpoint_address.is_empty() {
        return Err(WarpError::MissingField("config.peers[0].endpoint.v4"));
    }
    let interface_address = response.config.interface.addresses.v4;
    if interface_address.is_empty() {
        return Err(WarpError::MissingField("config.interface.addresses.v4"));
    }

    Ok(WarpRegistration {
        device_id: response.id,
        token: response.token,
        private_key: private_key_b64,
        peer_public_key: peer.public_key,
        endpoint_address,
        endpoint_port: WARP_ENDPOINT_PORT,
        local_address_v4: format!("{interface_address}/32"),
    })
}

fn build_client() -> Result<reqwest::Client, WarpError> {
    reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build().map_err(WarpError::ClientBuild)
}

/// Registers a brand-new, anonymous WARP device with Cloudflare: generates a
/// local X25519 keypair, POSTs the public half to Cloudflare's registration
/// API, and maps the response into a [`WarpRegistration`] ready to become a
/// WireGuard `ServerConfig`. See this module's doc comment for the API this
/// hits and why it's safe/appropriate to call with no credentials.
pub async fn register() -> Result<WarpRegistration, WarpError> {
    let secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
    let public = PublicKey::from(&secret);
    let private_key_b64 = STANDARD.encode(secret.to_bytes());
    let public_key_b64 = STANDARD.encode(public.as_bytes());

    let body = build_register_request(&public_key_b64);
    let client = build_client()?;
    let response = client
        .post(REG_BASE_URL)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header("CF-Client-Version", CF_CLIENT_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|source| WarpError::Request { url: REG_BASE_URL.to_string(), source })?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(parse_error_response(status, &text));
    }

    let parsed: RegisterResponse = response
        .json()
        .await
        .map_err(|source| WarpError::Deserialize { url: REG_BASE_URL.to_string(), source })?;

    build_registration(parsed, private_key_b64)
}

/// Deregisters a previously-registered WARP device. Any 2xx response is
/// treated as success (a real deregistration returns `204 No Content`).
pub async fn deregister(device_id: &str, token: &str) -> Result<(), WarpError> {
    let url = format!("{REG_BASE_URL}/{device_id}");
    let client = build_client()?;
    let response = client
        .delete(&url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header("CF-Client-Version", CF_CLIENT_VERSION)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .map_err(|source| WarpError::Request { url: url.clone(), source })?;

    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        let text = response.text().await.unwrap_or_default();
        Err(parse_error_response(status, &text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_request_body_has_expected_shape() {
        let req = build_register_request("dGVzdC1wdWJsaWMta2V5");
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["key"], "dGVzdC1wdWJsaWMta2V5");
        assert_eq!(value["install_id"], "");
        assert_eq!(value["fcm_token"], "");
        assert_eq!(value["tos"], TOS_TIMESTAMP);
        assert_eq!(value["type"], "Android");
        assert_eq!(value["locale"], "en_US");
    }

    #[test]
    fn strip_port_removes_trailing_placeholder_port() {
        assert_eq!(strip_port("162.159.192.2:0"), "162.159.192.2");
        assert_eq!(strip_port("162.159.192.2:2408"), "162.159.192.2");
    }

    #[test]
    fn strip_port_leaves_bare_address_unchanged() {
        assert_eq!(strip_port("162.159.192.2"), "162.159.192.2");
    }

    /// Realistic response shape, matching a real registration performed
    /// during development (see this crate's `README`/doc comment and the
    /// task's own confirmed `curl` transcript) -- includes the placeholder
    /// `:0` port on `endpoint.v4` and extra fields this crate doesn't model
    /// (`ipv6`/`host`/`ports`, `account`, etc.) to confirm `#[derive(Deserialize)]`
    /// tolerates unknown fields rather than requiring an exhaustive match.
    const REALISTIC_RESPONSE_JSON: &str = r#"{
        "id": "bd96bf2e-1234-4a12-9abc-1234567890ab",
        "token": "afc18650-abcd-4a12-9abc-0987654321ba",
        "account": {"id": "irrelevant"},
        "config": {
            "peers": [{
                "public_key": "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=",
                "endpoint": {
                    "v4": "162.159.192.2:0",
                    "v6": "[2606:4700:d0::a29f:c001]:0",
                    "host": "engage.cloudflareclient.com:2408",
                    "ports": [2408, 500, 1701, 4500]
                }
            }],
            "interface": {
                "addresses": {
                    "v4": "172.16.0.2",
                    "v6": "2606:4700:110:8a36:df5:awer:aa20:8a4d"
                }
            }
        },
        "policy": {"tunnel_protocol": "masque"},
        "warp_enabled": true
    }"#;

    #[test]
    fn parses_realistic_registration_response_end_to_end() {
        let parsed: RegisterResponse = serde_json::from_str(REALISTIC_RESPONSE_JSON).unwrap();
        let registration = build_registration(parsed, "local-private-key-b64".to_string()).unwrap();

        assert_eq!(registration.device_id, "bd96bf2e-1234-4a12-9abc-1234567890ab");
        assert_eq!(registration.token, "afc18650-abcd-4a12-9abc-0987654321ba");
        assert_eq!(registration.private_key, "local-private-key-b64");
        assert_eq!(registration.peer_public_key, "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=");
        // The ":0" placeholder port must be stripped, and the fixed WARP
        // port used instead -- not the (unusable) placeholder.
        assert_eq!(registration.endpoint_address, "162.159.192.2");
        assert_eq!(registration.endpoint_port, WARP_ENDPOINT_PORT);
        assert_eq!(registration.endpoint_port, 2408);
        assert_eq!(registration.local_address_v4, "172.16.0.2/32");
    }

    #[test]
    fn build_registration_errors_when_no_peers() {
        let response = RegisterResponse {
            id: "id".into(),
            token: "token".into(),
            config: RegisterConfig {
                peers: vec![],
                interface: RegisterInterface {
                    addresses: RegisterAddresses { v4: "172.16.0.2".into() },
                },
            },
        };
        let err = build_registration(response, "pk".to_string()).unwrap_err();
        assert!(matches!(err, WarpError::MissingField("config.peers[0]")));
    }

    #[test]
    fn error_response_surfaces_documented_api_message() {
        let body = r#"{"success":false,"errors":[{"code":1015,"message":"rate limited"}]}"#;
        let err = parse_error_response(reqwest::StatusCode::TOO_MANY_REQUESTS, body);
        let text = err.to_string();
        assert!(text.contains("rate limited"), "expected message text in: {text}");
        assert!(text.contains("1015"), "expected error code in: {text}");
        assert!(text.contains("429"), "expected status code in: {text}");
    }

    #[test]
    fn error_response_joins_multiple_errors() {
        let body = r#"{"success":false,"errors":[
            {"code":1,"message":"first problem"},
            {"code":2,"message":"second problem"}
        ]}"#;
        let err = parse_error_response(reqwest::StatusCode::BAD_REQUEST, body);
        let text = err.to_string();
        assert!(text.contains("first problem"));
        assert!(text.contains("second problem"));
    }

    #[test]
    fn error_response_falls_back_to_raw_body_when_not_the_documented_shape() {
        let err = parse_error_response(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
        assert!(err.to_string().contains("Internal Server Error"));
    }

    #[test]
    fn error_response_falls_back_to_placeholder_when_body_empty() {
        let err = parse_error_response(reqwest::StatusCode::SERVICE_UNAVAILABLE, "");
        assert!(err.to_string().contains("no error details provided"));
    }
}
