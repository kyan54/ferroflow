//! Streaming/AI-service "unlock" probing -- checks whether well-known
//! subscription services are reachable/available through the currently
//! running sing-box instance's local `mixed` inbound (see
//! `CoreManager::check_unlock`), mirroring what community region-unlock
//! checker tools (the various Netflix/Disney+/ChatGPT "media unlock test"
//! shell scripts passed around in the proxy community) do: make a handful of
//! unauthenticated HTTP requests to each provider's public-facing edge and
//! classify the response.
//!
//! **These probes are inherently best-effort.** They rely on specific
//! title ids, JSON field names, HTML substrings, or redirect shapes that
//! providers can and do change without notice -- that fragility is a known,
//! accepted property of this whole technique (every open-source unlock
//! checker needs periodic maintenance for exactly this reason), not
//! specific to this implementation. Where a probe's exact response shape
//! isn't nailed down by public documentation, the parsing here is
//! deliberately defensive (scans a small family of plausible keys/
//! substrings) so a minor provider-side change degrades to
//! `UnlockStatus::Unknown` rather than panicking or silently misreporting.
//!
//! Every request goes through the caller-supplied local proxy port via a
//! plain HTTP-proxy `reqwest::Proxy` -- the local inbound is sing-box's
//! `mixed` type (HTTP CONNECT *and* SOCKS5 on one port, see
//! `config::build_inbound`), and `reqwest` talking to it as an HTTP proxy
//! is enough to tunnel both plain HTTP and HTTPS (via CONNECT) requests.

use std::time::Duration;

use reqwest::{Client, Proxy, StatusCode};
use shared_types::{UnlockResult, UnlockStatus};

/// Per-request timeout. Generous enough for a real proxied round trip to a
/// far-away edge server, short enough that one unreachable service doesn't
/// stall the whole `unlock_check` call for long -- probes run concurrently
/// (see `check_all`), so the wall-clock cost of the whole batch is roughly
/// this times the number of sequential requests the slowest single probe
/// makes (at most two, today).
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

const DESKTOP_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

fn client_following(port: u16) -> Result<Client, reqwest::Error> {
    let proxy = Proxy::all(format!("http://127.0.0.1:{port}"))?;
    Client::builder().proxy(proxy).timeout(PROBE_TIMEOUT).build()
}

/// Same as `client_following`, but never follows redirects -- for probes
/// (Spotify, Prime Video) that read the *redirect itself* (its `Location`
/// header) as the signal, rather than the page it points to.
fn client_no_redirect(port: u16) -> Result<Client, reqwest::Error> {
    let proxy = Proxy::all(format!("http://127.0.0.1:{port}"))?;
    Client::builder()
        .proxy(proxy)
        .timeout(PROBE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

fn result(service: &str, status: UnlockStatus, region: Option<String>, detail: Option<String>) -> UnlockResult {
    UnlockResult { service: service.to_string(), status, region, detail }
}

fn unlocked(service: &str, region: Option<String>, detail: Option<String>) -> UnlockResult {
    result(service, UnlockStatus::Unlocked, region, detail)
}

fn locked(service: &str, region: Option<String>) -> UnlockResult {
    result(service, UnlockStatus::Locked, region, None)
}

fn unknown(service: &str, detail: impl Into<String>) -> UnlockResult {
    result(service, UnlockStatus::Unknown, None, Some(detail.into()))
}

fn error(service: &str, detail: impl Into<String>) -> UnlockResult {
    result(service, UnlockStatus::Error, None, Some(detail.into()))
}

/// Runs every catalog probe concurrently against the given local proxy port
/// and returns one `UnlockResult` per service, in a fixed catalog order.
/// Never fails outright -- an individual probe's own connection/parse
/// failures are reported as that entry's `UnlockStatus::Error`/`Unknown`,
/// not propagated as an `Err` for the whole batch.
pub async fn check_all(port: u16) -> Vec<UnlockResult> {
    let (netflix, disney_plus, youtube_premium, chatgpt, spotify, prime_video) = tokio::join!(
        probe_netflix(port),
        probe_disney_plus(port),
        probe_youtube_premium(port),
        probe_chatgpt(port),
        probe_spotify(port),
        probe_prime_video(port),
    );
    vec![netflix, disney_plus, youtube_premium, chatgpt, spotify, prime_video]
}

// --- Netflix -----------------------------------------------------------
//
// Standard community-script technique: anonymous (no session cookie)
// requests to a title's page return HTTP 404 (Netflix's own not-found page)
// when that title isn't licensed for the visitor's detected region, or 200
// when it is -- no redirect involved. Two title ids are checked:
// - `NETFLIX_ORIGINALS_TITLE` is a Netflix Original available in every
//   region Netflix operates in at all, so a 200 there alone means at least
//   the Originals-only catalog is reachable ("soft" unlock, e.g. via a
//   Netflix region that has no local licensing deals).
// - `NETFLIX_FULL_CATALOG_TITLE` is a broadly-but-not-universally-licensed
//   title, so a 200 there means the full (non-Originals-only) catalog is
//   reachable.
// Netflix's anonymous title-page response carries no reliable
// region/country signal, so `region` is always `None` here -- this probe
// distinguishes reachability tiers, not which specific region.
const NETFLIX_FULL_CATALOG_TITLE: u64 = 70143836;
const NETFLIX_ORIGINALS_TITLE: u64 = 81280792;

async fn probe_netflix(port: u16) -> UnlockResult {
    const NAME: &str = "Netflix";
    let client = match client_following(port) {
        Ok(c) => c,
        Err(e) => return error(NAME, format!("failed to build proxied client: {e}")),
    };

    let full_status = match client
        .get(format!("https://www.netflix.com/title/{NETFLIX_FULL_CATALOG_TITLE}"))
        .send()
        .await
    {
        Ok(resp) => resp.status(),
        Err(e) => return error(NAME, format!("request failed: {e}")),
    };
    if full_status == StatusCode::OK {
        return unlocked(NAME, None, Some("Full catalog".into()));
    }

    let originals_status = match client
        .get(format!("https://www.netflix.com/title/{NETFLIX_ORIGINALS_TITLE}"))
        .send()
        .await
    {
        Ok(resp) => resp.status(),
        Err(e) => return error(NAME, format!("request failed: {e}")),
    };
    if originals_status == StatusCode::OK {
        return unlocked(NAME, None, Some("Originals library only".into()));
    }

    if full_status == StatusCode::NOT_FOUND && originals_status == StatusCode::NOT_FOUND {
        return locked(NAME, None);
    }

    unknown(NAME, format!("unexpected response ({full_status}, {originals_status})"))
}

// --- Disney+ -------------------------------------------------------------
//
// Disney+'s public, unauthenticated "geo" API
// (`global.edge.bamgrid.com/geo`) is what the app itself calls before
// login to decide whether to show a signup flow or a "not available here"
// page -- a well-known target in community unlock checkers for exactly that
// reason. Its exact JSON shape isn't publicly pinned down to one schema
// version, so this scans for the first plausible "is this location
// supported" boolean and country-code string anywhere in the response
// (`find_bool`/`find_string`) rather than binding to one exact path.

async fn probe_disney_plus(port: u16) -> UnlockResult {
    const NAME: &str = "Disney+";
    let client = match client_following(port) {
        Ok(c) => c,
        Err(e) => return error(NAME, format!("failed to build proxied client: {e}")),
    };

    let resp = match client.get("https://global.edge.bamgrid.com/geo").send().await {
        Ok(r) => r,
        Err(e) => return error(NAME, format!("request failed: {e}")),
    };
    if !resp.status().is_success() {
        return unknown(NAME, format!("geo endpoint returned HTTP {}", resp.status()));
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return unknown(NAME, format!("could not parse geo response: {e}")),
    };

    let supported = find_bool(&body, &["inSupportedLocation", "in_supported_location", "supported"]);
    let region = find_string(&body, &["countryCode", "country_code"]);

    match supported {
        Some(true) => unlocked(NAME, region, None),
        Some(false) => locked(NAME, region),
        None => unknown(NAME, "could not determine availability from geo response"),
    }
}

// --- YouTube Premium -------------------------------------------------------
//
// Community-script technique: fetch the Premium marketing page and look for
// one of the phrases YouTube shows when Premium isn't offered in the
// visitor's country, versus a normal signup page. The page also embeds a
// `"GL":"XX"` (geolocation) field in its inline config JSON that's used here
// as a best-effort region signal when present.

async fn probe_youtube_premium(port: u16) -> UnlockResult {
    const NAME: &str = "YouTube Premium";
    let client = match client_following(port) {
        Ok(c) => c,
        Err(e) => return error(NAME, format!("failed to build proxied client: {e}")),
    };

    let resp = match client
        .get("https://www.youtube.com/premium")
        .header("User-Agent", DESKTOP_UA)
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return error(NAME, format!("request failed: {e}")),
    };
    let status = resp.status();
    let body = match resp.text().await {
        Ok(b) => b,
        Err(e) => return unknown(NAME, format!("could not read response body: {e}")),
    };

    if !status.is_success() {
        return unknown(NAME, format!("HTTP {status}"));
    }

    let region = extract_gl_region(&body);
    let lower = body.to_lowercase();
    const BLOCKED_MARKERS: &[&str] =
        &["isn't available in your country", "not available in your country", "premium is not available"];
    if BLOCKED_MARKERS.iter().any(|m| lower.contains(m)) {
        return locked(NAME, region);
    }

    unlocked(NAME, region, None)
}

fn extract_gl_region(body: &str) -> Option<String> {
    let idx = body.find("\"GL\":\"")?;
    let rest = body.get(idx + 6..)?;
    let end = rest.find('"')?;
    let code = rest.get(..end)?;
    (code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic())).then(|| code.to_uppercase())
}

// --- ChatGPT (OpenAI) ------------------------------------------------------
//
// OpenAI's Cloudflare-fronted `ios.chat.openai.com` returns HTTP 403 with a
// body containing `"unsupported_country"` for requests originating from a
// country OpenAI doesn't serve -- a technique widely referenced in
// community region-check scripts. Region is read separately from
// `chat.openai.com/cdn-cgi/trace`, Cloudflare's standard per-request debug
// endpoint (present on any Cloudflare-fronted site, not an OpenAI-specific
// API) which reports the exit IP's detected country as a plain `loc=XX`
// line.

async fn probe_chatgpt(port: u16) -> UnlockResult {
    const NAME: &str = "ChatGPT";
    let client = match client_following(port) {
        Ok(c) => c,
        Err(e) => return error(NAME, format!("failed to build proxied client: {e}")),
    };

    let resp = match client.get("https://ios.chat.openai.com/").send().await {
        Ok(r) => r,
        Err(e) => return error(NAME, format!("request failed: {e}")),
    };
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let region = probe_cf_trace_region(&client).await;

    if status == StatusCode::FORBIDDEN && body.to_lowercase().contains("unsupported_country") {
        return locked(NAME, region);
    }
    if status.is_success() {
        return unlocked(NAME, region, None);
    }
    unknown(NAME, format!("HTTP {status}"))
}

async fn probe_cf_trace_region(client: &Client) -> Option<String> {
    let resp = client.get("https://chat.openai.com/cdn-cgi/trace").send().await.ok()?;
    let body = resp.text().await.ok()?;
    body.lines().find_map(|l| l.strip_prefix("loc=")).map(|s| s.to_uppercase())
}

// --- Spotify ---------------------------------------------------------------
//
// Unauthenticated visits to Spotify's signup page 302-redirect to a
// locale-prefixed path (e.g. `https://www.spotify.com/us/signup/`) -- the
// two-letter segment right after the host is the region this probe reports.
// A non-redirecting response is treated as inconclusive rather than
// assumed-locked, since Spotify's exact redirect behavior can vary.

async fn probe_spotify(port: u16) -> UnlockResult {
    const NAME: &str = "Spotify";
    let client = match client_no_redirect(port) {
        Ok(c) => c,
        Err(e) => return error(NAME, format!("failed to build proxied client: {e}")),
    };

    let resp = match client.get("https://www.spotify.com/signup/").send().await {
        Ok(r) => r,
        Err(e) => return error(NAME, format!("request failed: {e}")),
    };
    let status = resp.status();

    if status.is_redirection() {
        let location = resp.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok());
        if let Some(region) = location.and_then(extract_locale_segment) {
            return unlocked(NAME, Some(region), None);
        }
        return unknown(NAME, "redirected but could not determine a region from the redirect");
    }
    if status.is_success() {
        return unlocked(NAME, None, None);
    }
    if status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN {
        return locked(NAME, None);
    }
    unknown(NAME, format!("HTTP {status}"))
}

fn extract_locale_segment(location: &str) -> Option<String> {
    let after_host = location.split("spotify.com/").nth(1)?;
    let segment = after_host.split('/').next()?;
    (segment.len() == 2 && segment.chars().all(|c| c.is_ascii_alphabetic())).then(|| segment.to_uppercase())
}

// --- Amazon Prime Video ------------------------------------------------
//
// An unauthenticated visit to `primevideo.com` either stays on
// `primevideo.com` (serving a signup/browse page -- available) or
// redirects out to a plain `amazon.com` "this isn't available in your
// country yet" page -- a distinction referenced in community unlock
// checkers. No reliable anonymous region signal here, so `region` is
// always `None`.

async fn probe_prime_video(port: u16) -> UnlockResult {
    const NAME: &str = "Prime Video";
    let client = match client_no_redirect(port) {
        Ok(c) => c,
        Err(e) => return error(NAME, format!("failed to build proxied client: {e}")),
    };

    let resp = match client.get("https://www.primevideo.com/").send().await {
        Ok(r) => r,
        Err(e) => return error(NAME, format!("request failed: {e}")),
    };
    let status = resp.status();

    if status.is_redirection() {
        let location =
            resp.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("");
        if location.contains("amazon.com") && !location.contains("primevideo") {
            return locked(NAME, None);
        }
        return unlocked(NAME, None, None);
    }
    if status.is_success() {
        return unlocked(NAME, None, None);
    }
    unknown(NAME, format!("HTTP {status}"))
}

// --- shared JSON scanning helpers ------------------------------------------

/// Recursively searches a JSON value for the first of `keys` whose value is
/// a bool, checking the current object's own keys before descending into
/// nested objects/arrays (breadth-first-ish preference for shallower
/// matches). Used where the exact schema of a third-party response isn't
/// pinned down (see `probe_disney_plus`).
fn find_bool(value: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    match value {
        serde_json::Value::Object(map) => {
            for k in keys {
                if let Some(b) = map.get(*k).and_then(|v| v.as_bool()) {
                    return Some(b);
                }
            }
            map.values().find_map(|v| find_bool(v, keys))
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(|v| find_bool(v, keys)),
        _ => None,
    }
}

/// Same as `find_bool`, for a string-valued field.
fn find_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for k in keys {
                if let Some(s) = map.get(*k).and_then(|v| v.as_str()) {
                    return Some(s.to_string());
                }
            }
            map.values().find_map(|v| find_string(v, keys))
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(|v| find_string(v, keys)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_gl_region_reads_two_letter_code() {
        let body = r#"{"INNERTUBE_CONTEXT":{},"GL":"US","HL":"en"}"#;
        assert_eq!(extract_gl_region(body), Some("US".to_string()));
    }

    #[test]
    fn extract_gl_region_ignores_missing_field() {
        assert_eq!(extract_gl_region(r#"{"HL":"en"}"#), None);
    }

    #[test]
    fn extract_locale_segment_reads_country_code() {
        assert_eq!(
            extract_locale_segment("https://www.spotify.com/us/signup/"),
            Some("US".to_string())
        );
    }

    #[test]
    fn extract_locale_segment_rejects_non_locale_paths() {
        assert_eq!(extract_locale_segment("https://www.spotify.com/signup/confirm/"), None);
        assert_eq!(extract_locale_segment("https://www.spotify.com/"), None);
    }

    #[test]
    fn find_bool_finds_nested_key() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"client":{"inSupportedLocation":true}}"#).unwrap();
        assert_eq!(find_bool(&value, &["inSupportedLocation"]), Some(true));
    }

    #[test]
    fn find_string_finds_nested_key() {
        let value: serde_json::Value = serde_json::from_str(r#"{"location":{"countryCode":"US"}}"#).unwrap();
        assert_eq!(find_string(&value, &["countryCode"]), Some("US".to_string()));
    }

    #[tokio::test]
    async fn check_all_returns_one_result_per_catalog_service() {
        // No real proxy running (bogus port, nothing bound) -- every probe's
        // `client.get(...).send()` should fail fast with a connection error,
        // exercising the `error(...)` path end-to-end without needing real
        // network access, while still confirming the catalog shape (six
        // services, in order) and that a batch never panics/short-circuits.
        let results = check_all(1).await;
        let names: Vec<&str> = results.iter().map(|r| r.service.as_str()).collect();
        assert_eq!(
            names,
            vec!["Netflix", "Disney+", "YouTube Premium", "ChatGPT", "Spotify", "Prime Video"]
        );
        for r in &results {
            assert_eq!(r.status, UnlockStatus::Error, "{}: expected Error against an unreachable proxy port, got {:?}", r.service, r.status);
        }
    }
}
