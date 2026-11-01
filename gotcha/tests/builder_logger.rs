//! The builder installs a tracing subscriber only on request. The global subscriber is
//! process-wide, so this file holds a single test.

use gotcha::{ConfigWrapper, EmptyConfig, Gotcha, ServerConfig};

#[tokio::test]
async fn builder_installs_the_default_subscriber_only_on_request() {
    let config = || ConfigWrapper {
        app: EmptyConfig {},
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
    };
    drop(Gotcha::new().config(config()).bind().await.unwrap());
    assert!(
        !gotcha::tracing::dispatcher::has_been_set(),
        "builder applications keep their own logging setup"
    );
    drop(Gotcha::new().config(config()).with_default_logger().bind().await.unwrap());
    assert!(gotcha::tracing::dispatcher::has_been_set());
    // Another application keeps the installed subscriber instead of failing to start.
    drop(Gotcha::new().config(config()).with_default_logger().bind().await.unwrap());
}
