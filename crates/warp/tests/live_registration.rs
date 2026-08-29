//! Real, live round-trip against Cloudflare's actual WARP registration API
//! -- not a fixture, not a mock. This is deliberately `#[ignore]`d: it makes
//! a real external network call against a rate-limit-conscious public
//! service, so it must never run as part of a normal `cargo test`/CI sweep.
//!
//! Run manually with:
//! `cargo test -p warp --test live_registration -- --ignored --nocapture`
//!
//! The test registers one real anonymous device and immediately deregisters
//! it in the same run (a `Drop`-style best-effort cleanup would be nicer,
//! but async cleanup in `Drop` isn't straightforward -- explicit cleanup at
//! the end of the test body is simpler and the test is short enough that a
//! panic between register/deregister is unlikely; if this ever does leave a
//! device registered, it's an anonymous throwaway device on Cloudflare's
//! side with no account attached, not a resource leak that matters).

#[tokio::test]
#[ignore = "hits the real Cloudflare WARP API -- run manually, not part of normal CI/test sweeps"]
async fn real_register_then_deregister_round_trip() {
    let registration = warp::register().await.expect("live registration should succeed");

    assert!(!registration.device_id.is_empty());
    assert!(!registration.token.is_empty());
    assert!(!registration.private_key.is_empty());
    assert!(!registration.peer_public_key.is_empty());
    assert!(!registration.endpoint_address.is_empty());
    assert_eq!(registration.endpoint_port, warp::WARP_ENDPOINT_PORT);
    assert!(registration.local_address_v4.ends_with("/32"));

    println!("live registration succeeded: {registration:?}");

    warp::deregister(&registration.device_id, &registration.token)
        .await
        .expect("live deregistration should succeed");

    println!("live deregistration succeeded for device {}", registration.device_id);
}
