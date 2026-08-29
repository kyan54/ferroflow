//! sing-box TUN inbound generation, for `ProxyModeType::Tun`.
//!
//! MVP-scoped (mirrors the shape, not full feature parity with the Electron
//! reference's TUN builder): no auto-detect-interface, no DNS hijack rules,
//! no `route_exclude_address`, no per-platform tuning (e.g. `gso` on Linux).
//! Just enough for sing-box to actually establish a working TUN device once
//! it's spawned by a privileged helper (root/SYSTEM/ambient caps — a plain
//! unprivileged process can't create a TUN interface, which is the whole
//! reason TUN mode routes through `helper-client` instead of `process.rs`).

use serde_json::{json, Value};

/// Tag of the generated TUN inbound.
pub const TUN_INBOUND_TAG: &str = "tun-in";

/// IPv4 address (with prefix) assigned to the TUN interface. A `/30` gives
/// exactly 2 usable host addresses (network + broadcast reserved) — sing-box
/// only needs one for the interface itself, so a `/30` is the smallest
/// conventional subnet and matches sing-box's own examples/docs for a
/// single-interface TUN setup. `172.19.x.x` is inside the private
/// `172.16.0.0/12` block and picked arbitrarily to avoid colliding with the
/// far more commonly used `172.17.0.0/16`/`172.18.0.0/16` (common Docker
/// default bridge ranges) on a dev machine that also runs containers.
const TUN_INET4_ADDRESS: &str = "172.19.0.1/30";

/// sing-box's own documented default TUN MTU.
const TUN_MTU: u32 = 9000;

/// Builds the sing-box TUN inbound stanza for `interface_name`. `auto_route`
/// and `strict_route` are both `true` so sing-box takes over the system's
/// default route for the interface's lifetime (this is the entire point of
/// TUN mode — whole-system traffic capture, not just apps that support a
/// SOCKS/HTTP proxy). `stack: "system"` is the simplest of sing-box's three
/// TUN stack implementations (`system`/`gvisor`/`mixed`) — no userspace
/// netstack, relies on the OS's own TCP/IP stack, which is sufficient for
/// MVP and avoids pulling in gVisor's extra complexity/deps.
pub fn build_tun_inbound(interface_name: &str) -> Value {
    json!({
        "type": "tun",
        "tag": TUN_INBOUND_TAG,
        "interface_name": interface_name,
        "inet4_address": TUN_INET4_ADDRESS,
        "mtu": TUN_MTU,
        "auto_route": true,
        "strict_route": true,
        "stack": "system",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tun_inbound_has_required_fields() {
        let inbound = build_tun_inbound("ferroflow-tun0");
        assert_eq!(inbound["type"], "tun");
        assert_eq!(inbound["tag"], TUN_INBOUND_TAG);
        assert_eq!(inbound["interface_name"], "ferroflow-tun0");
        assert_eq!(inbound["inet4_address"], TUN_INET4_ADDRESS);
        assert_eq!(inbound["mtu"], TUN_MTU);
        assert_eq!(inbound["auto_route"], true);
        assert_eq!(inbound["strict_route"], true);
        assert_eq!(inbound["stack"], "system");
    }

    #[test]
    fn tun_inbound_field_types_are_correct() {
        let inbound = build_tun_inbound("tun0");
        assert!(inbound["type"].is_string());
        assert!(inbound["tag"].is_string());
        assert!(inbound["interface_name"].is_string());
        assert!(inbound["inet4_address"].is_string());
        assert!(inbound["mtu"].is_u64());
        assert!(inbound["auto_route"].is_boolean());
        assert!(inbound["strict_route"].is_boolean());
        assert!(inbound["stack"].is_string());
    }
}
