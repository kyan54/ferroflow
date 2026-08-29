//! Curated catalog + download mechanics for sing-box GeoIP/GeoSite `.srs`
//! rule-set files.
//!
//! Real sing-box GUIs (including the Electron app this is a rewrite of,
//! "FlowZ") ship curated GeoIP/GeoSite rule-set files that users reference
//! by name instead of typing thousands of domains by hand. SagerNet
//! (sing-box's author) publishes one small, individual `.srs` file per
//! category on a dedicated `rule-set` branch of each of its
//! `sing-geosite`/`sing-geoip` repos (not a GitHub release) -- see
//! `catalog::resource_url`'s doc comment for the exact URL shape.
//!
//! This crate is deliberately decoupled from `shared-types`/`core-manager`:
//! it knows nothing about `UserConfig`, `RoutingRule`, or the sing-box
//! config JSON shape those wire into (see `core_manager::config` for that
//! wiring) -- just "given a category/name, what's the URL" and "given a
//! URL, download it to disk and report what happened".

pub mod catalog;
pub mod download;
mod time;

pub use catalog::{builtin_catalog, resource_url, CatalogEntry, ResourceCategory};
pub use download::{download, DownloadedResource, RuleResourceError};
