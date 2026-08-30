//! Subscription-link import: fetching a provider's subscription URL and
//! parsing the base64-or-plaintext body it returns into
//! `shared_types::ServerConfig`s.
//!
//! Split into three modules on purpose:
//! - `parse` is pure and side-effect-free (no network, no filesystem), so
//!   it's fully unit-testable -- see its test module for realistic
//!   `vless://`/`trojan://`/`ss://`/`vmess://` fixtures.
//! - `fetch` does the actual HTTP GET and is intentionally kept tiny and
//!   boring, since it can't reasonably be unit-tested without standing up a
//!   mock HTTP server.
//! - `clash` is also pure and side-effect-free, converting a Clash-style
//!   YAML config's `proxies:` list into `ServerConfig`s -- a second input
//!   shape distinct from share-links/subscription bodies, used by the
//!   file-import path.

pub mod clash;
pub mod fetch;
pub mod parse;

pub use clash::parse_clash_yaml;
pub use fetch::{fetch_subscription, FetchError};
pub use parse::{decode_subscription_body, generate_share_url, parse_subscription_body, parse_uri};
