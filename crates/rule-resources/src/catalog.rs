//! The curated built-in catalog of GeoIP/GeoSite rule-set resources, plus
//! the URL-building logic shared by both catalog and custom (user-typed)
//! resources.
//!
//! SagerNet (sing-box's author) publishes one small, individual `.srs` file
//! per category on a dedicated `rule-set` branch of each of its
//! `sing-geosite`/`sing-geoip` repos (not a GitHub release) -- confirmed
//! directly against the real endpoints:
//! `https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-<name>.srs`
//! `https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-<name>.srs`
//! Each repo's `rule-set` branch holds well over a thousand files (every
//! GeoSite/GeoIP category upstream ships), far too many to enumerate here --
//! `builtin_catalog` is a small, deliberately curated subset of the most
//! commonly useful ones; any other valid upstream filename can still be
//! added by a user via the "custom" download flow
//! (`rule_resources_download_custom` on the `src-tauri` side), which takes
//! an arbitrary name/URL directly rather than requiring it appear here.

use serde::{Deserialize, Serialize};

/// Which upstream repo (and file-prefix) a rule-set resource comes from.
/// Distinct from `shared_types::RuleResourceCategory` -- see that type's doc
/// comment for why the two aren't unified into one type shared across
/// crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceCategory {
    Geosite,
    GeoIp,
}

impl ResourceCategory {
    fn repo_name(self) -> &'static str {
        match self {
            ResourceCategory::Geosite => "sing-geosite",
            ResourceCategory::GeoIp => "sing-geoip",
        }
    }

    fn file_prefix(self) -> &'static str {
        match self {
            ResourceCategory::Geosite => "geosite",
            ResourceCategory::GeoIp => "geoip",
        }
    }
}

/// One entry of the curated built-in catalog (`builtin_catalog`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    /// Bare category name, e.g. `"netflix"` -- no `geosite-`/`geoip-` prefix
    /// or `.srs` suffix (those are added by `resource_url`).
    pub name: String,
    pub category: ResourceCategory,
    /// Human-readable label for display, e.g. `"Netflix"`.
    pub label: String,
}

fn entry(name: &str, category: ResourceCategory, label: &str) -> CatalogEntry {
    CatalogEntry { name: name.to_string(), category, label: label.to_string() }
}

/// A small, curated set of commonly useful GeoIP/GeoSite rule-sets -- not
/// meant to be exhaustive (SagerNet's real `rule-set` branches hold well
/// over a thousand files each). Users who want something not listed here
/// can still add it by exact upstream filename via the "custom" download
/// flow.
pub fn builtin_catalog() -> Vec<CatalogEntry> {
    use ResourceCategory::{GeoIp, Geosite};
    vec![
        entry("cn", Geosite, "China mainland domains"),
        entry("geolocation-!cn", Geosite, "Non-China domains"),
        entry("cn", GeoIp, "China mainland IPs"),
        entry("private", GeoIp, "Private / LAN IP ranges"),
        entry("category-ads-all", Geosite, "Ad & tracking domains"),
        entry("netflix", Geosite, "Netflix"),
        entry("youtube", Geosite, "YouTube"),
        entry("google", Geosite, "Google"),
        entry("github", Geosite, "GitHub"),
        entry("openai", Geosite, "OpenAI"),
        entry("telegram", Geosite, "Telegram"),
        entry("tiktok", Geosite, "TikTok"),
        entry("disney", Geosite, "Disney+"),
        entry("spotify", Geosite, "Spotify"),
        entry("twitter", Geosite, "Twitter / X"),
        entry("facebook", Geosite, "Facebook"),
        entry("instagram", Geosite, "Instagram"),
        entry("microsoft", Geosite, "Microsoft"),
        entry("apple", Geosite, "Apple"),
        entry("amazon", Geosite, "Amazon"),
    ]
}

/// Builds the real upstream download URL for `category`/`name`
/// (`https://raw.githubusercontent.com/SagerNet/<repo>/rule-set/<prefix>-<name>.srs`),
/// optionally prepended with `github_accel_prefix` -- an opaque string
/// prefix (e.g. a "GitHub 加速"/GitHub-acceleration mirror like
/// `"https://ghproxy.com/"`) this function does not validate or special-case
/// in any way, it's simply string-concatenated in front of the real URL. A
/// `None` or empty prefix fetches directly from `raw.githubusercontent.com`.
pub fn resource_url(category: ResourceCategory, name: &str, github_accel_prefix: Option<&str>) -> String {
    let base = format!(
        "https://raw.githubusercontent.com/SagerNet/{}/rule-set/{}-{}.srs",
        category.repo_name(),
        category.file_prefix(),
        name
    );
    match github_accel_prefix {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}{base}"),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_url_geosite_no_prefix() {
        assert_eq!(
            resource_url(ResourceCategory::Geosite, "netflix", None),
            "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-netflix.srs"
        );
    }

    #[test]
    fn resource_url_geoip_no_prefix() {
        assert_eq!(
            resource_url(ResourceCategory::GeoIp, "cn", None),
            "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-cn.srs"
        );
    }

    #[test]
    fn resource_url_with_accel_prefix_is_prepended_verbatim() {
        assert_eq!(
            resource_url(ResourceCategory::Geosite, "netflix", Some("https://ghproxy.com/")),
            "https://ghproxy.com/https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-netflix.srs"
        );
    }

    #[test]
    fn resource_url_empty_prefix_is_treated_as_none() {
        assert_eq!(
            resource_url(ResourceCategory::Geosite, "netflix", Some("")),
            resource_url(ResourceCategory::Geosite, "netflix", None)
        );
    }

    #[test]
    fn resource_url_geoip_with_accel_prefix() {
        assert_eq!(
            resource_url(ResourceCategory::GeoIp, "private", Some("https://mirror.example/")),
            "https://mirror.example/https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-private.srs"
        );
    }

    #[test]
    fn builtin_catalog_is_a_small_curated_set() {
        let catalog = builtin_catalog();
        assert!(catalog.len() >= 15 && catalog.len() <= 25, "expected a curated ~15-20 entry catalog, got {}", catalog.len());
    }

    #[test]
    fn builtin_catalog_has_no_duplicate_name_category_pairs() {
        let catalog = builtin_catalog();
        let mut seen = std::collections::HashSet::new();
        for e in &catalog {
            assert!(seen.insert((e.name.clone(), e.category)), "duplicate catalog entry: {:?}/{}", e.category, e.name);
        }
    }

    #[test]
    fn builtin_catalog_entries_have_non_empty_fields() {
        for e in builtin_catalog() {
            assert!(!e.name.is_empty());
            assert!(!e.label.is_empty());
        }
    }
}
