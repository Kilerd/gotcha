//! `default_logger` installs the subscriber that `GotchaApp` and `with_default_logger` install,
//! for applications that serve an assembled router themselves. The global subscriber is
//! process-wide, so this file holds a single test.

#[test]
fn default_logger_installs_the_global_subscriber() {
    assert!(!gotcha::tracing::dispatcher::has_been_set());
    gotcha::default_logger().unwrap();
    assert!(gotcha::tracing::dispatcher::has_been_set());
    // A second call keeps the installed subscriber instead of failing.
    gotcha::default_logger().unwrap();
}
