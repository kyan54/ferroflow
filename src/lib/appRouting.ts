// Curated catalog + region presets backing `AppRoutingView` -- a friendlier
// UI layer on top of the existing `RoutingRule`/`rule-resources` infra (see
// `RulesView`/`RuleResourcesView`), not a parallel storage mechanism. Every
// toggle here just creates/updates/removes a `RoutingRule` with
// `matchType: "ruleSet"` in `UserConfig.rules`, tagged by a deterministic
// `id` convention (see `appRoutingRuleId`/`PRESET_RULE_PREFIX` below) so this
// page can read its own state back out of `config.rules` on load instead of
// needing a dedicated marker field on `RoutingRule` itself.

import type { RoutingRule, RuleOutbound, RuleResourceCategory } from "../types";

/** Maps a `RuleResourceCategory` (wire value) to the upstream file-prefix
 * SagerNet's `sing-geosite`/`sing-geoip` repos use -- mirrors
 * `commands::rule_resources::category_file_prefix` on the Rust side exactly
 * (note "geoIp" the wire tag vs. "geoip" the file prefix -- these differ). */
const CATEGORY_FILE_PREFIX: Record<RuleResourceCategory, string> = {
  geosite: "geosite",
  geoIp: "geoip",
};

/** Builds a `RuleResourceInfo.id` -- `"<file-prefix>-<name>"` (e.g.
 * `"geosite-netflix"`, `"geoip-cn"`). Must exactly match
 * `commands::rule_resources::resource_id` on the Rust side (see that
 * function's doc comment for why bare `name` can't be used: the builtin
 * catalog has entries that share a `name` across both categories, e.g.
 * `"cn"` is both a GeoSite and a GeoIP entry). Used here to check whether a
 * resource this page wants to reference has already been downloaded, before
 * asking `rule_resources_download` to fetch it. */
export function ruleResourceId(category: RuleResourceCategory, name: string): string {
  return `${CATEGORY_FILE_PREFIX[category]}-${name}`;
}

/** Deterministic `RoutingRule.id` prefix for a rule this page manages (one
 * per app, keyed by the catalog entry's own `id`) -- lets the page find its
 * own rule back out of `config.rules` by id, and lets `rules_update`/
 * `rules_delete` target it directly instead of needing a search-by-name. */
const APP_ROUTING_RULE_PREFIX = "app-routing:";

export function appRoutingRuleId(appId: string): string {
  return `${APP_ROUTING_RULE_PREFIX}${appId}`;
}

/** Deterministic `RoutingRule.id` prefix for rules a region preset created
 * (`"preset:<presetId>:<index>"`) -- lets `applyRegionPreset` find and
 * replace only the *previous* preset's rules on a fresh apply, without
 * touching manual rules or `AppRoutingView` toggles living in the same
 * `config.rules` list. */
export const PRESET_RULE_PREFIX = "preset:";

export interface AppRoutingEntry {
  /** Stable id, also used as the `RoutingRule.id` suffix -- see `appRoutingRuleId`. */
  id: string;
  label: string;
  /** Bare GeoSite catalog name (see `rule_resources::builtin_catalog`), e.g. `"netflix"`. */
  geosite: string;
}

export interface AppRoutingCategory {
  id: string;
  label: string;
  apps: AppRoutingEntry[];
}

/** Curated built-in catalog of well-known apps/services a user can route
 * with one click, grouped the same way FlowZ's own "应用分流" (app routing)
 * page groups them. Each entry maps to an existing GeoSite rule-set already
 * in (or added to, for this feature) `rule_resources::builtin_catalog()` --
 * verified against SagerNet's real `sing-geosite` `rule-set` branch rather
 * than hand-listed domains, so this stays accurate as upstream's community
 * domain lists change. */
export const APP_ROUTING_CATALOG: AppRoutingCategory[] = [
  {
    id: "streaming",
    label: "Streaming",
    apps: [
      { id: "netflix", label: "Netflix", geosite: "netflix" },
      { id: "youtube", label: "YouTube", geosite: "youtube" },
      { id: "disneyplus", label: "Disney+", geosite: "disney" },
      { id: "spotify", label: "Spotify", geosite: "spotify" },
    ],
  },
  {
    id: "social",
    label: "Social",
    apps: [
      { id: "twitter", label: "Twitter / X", geosite: "twitter" },
      { id: "instagram", label: "Instagram", geosite: "instagram" },
      { id: "telegram", label: "Telegram", geosite: "telegram" },
      { id: "discord", label: "Discord", geosite: "discord" },
    ],
  },
  {
    id: "ai",
    label: "AI tools",
    apps: [
      { id: "openai", label: "ChatGPT / OpenAI", geosite: "openai" },
      { id: "anthropic", label: "Claude / Anthropic", geosite: "anthropic" },
      { id: "gemini", label: "Google Gemini", geosite: "google" },
    ],
  },
  {
    id: "gaming",
    label: "Gaming",
    apps: [
      { id: "steam", label: "Steam", geosite: "steam" },
      { id: "playstation", label: "PlayStation Network", geosite: "playstation" },
    ],
  },
  {
    id: "devtools",
    label: "Dev & productivity",
    apps: [
      { id: "github", label: "GitHub", geosite: "github" },
      { id: "dockerhub", label: "Docker Hub", geosite: "docker" },
      { id: "microsoft", label: "Microsoft", geosite: "microsoft" },
      { id: "apple", label: "Apple", geosite: "apple" },
    ],
  },
];

/** One rule a `RegionPreset` generates. `geositeNames`/`geoIpNames` become a
 * *single* `RoutingRule` with `matchType: "ruleSet"` and `values` holding
 * every listed resource's id -- sing-box's `rule_set` route-rule field
 * matches if traffic matches ANY of the listed rule-sets, so bundling e.g.
 * GeoSite `cn` + GeoIP `cn` into one rule is exactly "China (by domain or by
 * IP)", not a compound AND condition. */
export interface RegionPresetRuleSpec {
  name: string;
  outbound: RuleOutbound;
  geositeNames?: string[];
  geoIpNames?: string[];
}

export interface RegionPreset {
  id: string;
  label: string;
  description: string;
  /** `UserConfig.defaultOutbound` this preset sets -- see that field's doc
   * comment for why a preset needs to touch this rather than only `rules`. */
  defaultOutbound: RuleOutbound;
  rules: RegionPresetRuleSpec[];
  /** Only `true` for "Global proxy, no rules" -- every other preset only
   * replaces *its own* previously-applied rules (see `PRESET_RULE_PREFIX`),
   * leaving manual rules and `AppRoutingView` toggles alone. This one is
   * explicitly a "wipe everything" preset by design/name, so it clears the
   * whole `rules` array instead. */
  clearsAllRules: boolean;
}

export const REGION_PRESETS: RegionPreset[] = [
  {
    id: "cn-direct",
    label: "China direct, rest proxy",
    description:
      "Mainland China domains and IP ranges go direct; everything else goes through the proxy.",
    defaultOutbound: "proxy",
    clearsAllRules: false,
    rules: [
      {
        name: "China direct (preset)",
        outbound: "direct",
        geositeNames: ["cn"],
        geoIpNames: ["cn"],
      },
    ],
  },
  {
    id: "streaming-proxy",
    label: "Streaming via proxy, rest direct",
    description:
      "Popular streaming services (Netflix, YouTube, Disney+, Spotify) go through the proxy; everything else goes direct.",
    defaultOutbound: "direct",
    clearsAllRules: false,
    rules: [
      {
        name: "Streaming via proxy (preset)",
        outbound: "proxy",
        geositeNames: ["netflix", "youtube", "disney", "spotify"],
      },
    ],
  },
  {
    id: "ads-cn-direct",
    label: "Block ads, China direct, rest proxy",
    description:
      "Ad/tracking domains are blocked, mainland China traffic goes direct, and everything else goes through the proxy.",
    defaultOutbound: "proxy",
    clearsAllRules: false,
    rules: [
      { name: "Block ads (preset)", outbound: "block", geositeNames: ["category-ads-all"] },
      { name: "China direct (preset)", outbound: "direct", geositeNames: ["cn"], geoIpNames: ["cn"] },
    ],
  },
  {
    id: "global-proxy",
    label: "Global proxy, no rules",
    description: "Removes every routing rule (including app routing) and sends all traffic through the proxy.",
    defaultOutbound: "proxy",
    clearsAllRules: true,
    rules: [],
  },
];

/** Builds the `RoutingRule.values` array for one `RegionPresetRuleSpec` --
 * every listed GeoSite/GeoIP name's resource id, in a stable order. */
function presetRuleValues(spec: RegionPresetRuleSpec): string[] {
  return [
    ...(spec.geositeNames ?? []).map((name) => ruleResourceId("geosite", name)),
    ...(spec.geoIpNames ?? []).map((name) => ruleResourceId("geoIp", name)),
  ];
}

/** Every distinct `(category, name)` resource a preset's rules reference --
 * used to ensure each is downloaded before the rules referencing it are
 * saved (an undownloaded resource just gets silently dropped by
 * `core_manager::config::build_route_rules`, so this avoids saving a rule
 * that would silently do nothing). */
export function presetResourceRefs(preset: RegionPreset): { category: RuleResourceCategory; name: string }[] {
  const refs: { category: RuleResourceCategory; name: string }[] = [];
  for (const spec of preset.rules) {
    for (const name of spec.geositeNames ?? []) refs.push({ category: "geosite", name });
    for (const name of spec.geoIpNames ?? []) refs.push({ category: "geoIp", name });
  }
  return refs;
}

/** Builds the concrete `RoutingRule[]` a preset contributes, with
 * deterministic `preset:<presetId>:<index>` ids (see `PRESET_RULE_PREFIX`). */
export function buildPresetRules(preset: RegionPreset): RoutingRule[] {
  return preset.rules.map((spec, index) => ({
    id: `${PRESET_RULE_PREFIX}${preset.id}:${index}`,
    name: spec.name,
    enabled: true,
    matchType: "ruleSet",
    values: presetRuleValues(spec),
    outbound: spec.outbound,
  }));
}
