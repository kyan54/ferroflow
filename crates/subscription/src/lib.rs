//! Subscription-link import: fetching a provider's subscription URL and
//! parsing the base64-or-plaintext body it returns into
//! `shared_types::ServerConfig`s.
//!
//! Split into two modules on purpose:
//! - `parse` is pure and side-effect-free (no network, no filesystem), so
//!   it's fully unit-testable -- see its test module for realistic
//!   `vless://`/`trojan://`/`ss://`/`vmess://` fixtures.
//! - `fetch` does the actual HTTP GET and is intentionally kept tiny and
//!   boring, since it can't reasonably be unit-tested without standing up a
//!   mock HTTP server.

pub mod fetch;
pub mod parse;

pub use fetch::{fetch_subscription, FetchError};
pub use parse::{decode_subscription_body, parse_subscription_body, parse_uri};
