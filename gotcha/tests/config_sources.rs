//! Configuration descriptions must retain their behavior when cloned, restored, or used to start an app.

use gotcha::config::{ConfigBuilder, ConfigSource, ConfigState};
#[cfg(feature = "http1")]
use gotcha::{Gotcha, GotchaError};
use serde_json::{json, Value};

fn assert_roundtrip(builder: ConfigBuilder, expected: Value) {
    let state = builder.state();
    assert_eq!(builder.build::<Value>().unwrap(), expected);
    assert_eq!(ConfigBuilder::from_state(state).build::<Value>().unwrap(), expected);
}

#[test]
fn file_environment_order_and_variables_survive_state_roundtrip() {
    // Environment sources enumerate the process environment. Set fixtures before a child process
    // starts instead of mutating the environment while other Rust tests might be reading it.
    const CHILD: &str = "GOTCHA_CONFIG_SOURCES_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "file_environment_order_and_variables_survive_state_roundtrip", "--nocapture"])
            .env(CHILD, "1")
            .env("GOTCHAFIRST_VALUE", "first-env")
            .env("GOTCHASECOND_VALUE", "second-env")
            .env("GOTCHAVARS_TOKEN", "suffix")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.toml");
    let second = dir.path().join("second.toml");
    std::fs::write(&first, "value = 'first-file'").unwrap();
    std::fs::write(&second, "value = 'second-file'").unwrap();

    let builder = ConfigBuilder::new().file(&first);
    assert_roundtrip(builder.clone(), json!({"value": "first-file"}));
    let builder = builder.env("GOTCHAFIRST");
    assert_roundtrip(builder.clone(), json!({"value": "first-env"}));
    let builder = builder.file_optional(&second);
    assert_roundtrip(builder.clone(), json!({"value": "second-file"}));
    assert_roundtrip(builder.env("GOTCHASECOND"), json!({"value": "second-env"}));

    let variables = dir.path().join("variables.toml");
    std::fs::write(&variables, "base = 'root'\nvalue = '${base}/${GOTCHAVARS_TOKEN}'").unwrap();
    let builder = ConfigBuilder::new().file(variables);
    assert_roundtrip(builder.clone(), json!({"base": "root", "value": "${base}/${GOTCHAVARS_TOKEN}"}));
    assert_roundtrip(builder.enable_vars(), json!({"base": "root", "value": "root/suffix"}));
}

#[test]
fn restoring_a_missing_required_file_still_fails() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("required.toml");
    let builder = ConfigBuilder::new().file(&missing);
    let restored = ConfigBuilder::from_state(builder.state());
    for builder in [builder, restored] {
        let error = builder.build::<Value>().unwrap_err().to_string();
        assert!(error.contains("required configuration file"), "{error}");
        assert!(error.contains("required.toml"), "{error}");
    }
    assert_roundtrip(ConfigBuilder::new().file_optional(missing), json!({}));
}

#[test]
fn files_are_checked_at_build_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("later.toml");
    let required = ConfigBuilder::new().file(&path);
    let optional = ConfigBuilder::new().file_optional(&path);

    // Both builders were registered while the file was absent.
    std::fs::write(&path, "value = 'created-later'").unwrap();
    assert_roundtrip(required, json!({"value": "created-later"}));
    assert_roundtrip(optional, json!({"value": "created-later"}));

    let required = ConfigBuilder::new().file(&path);
    let restored = ConfigBuilder::from_state(required.state());
    let optional = ConfigBuilder::new().file_optional(&path);
    std::fs::remove_file(path).unwrap();
    assert!(required.build::<Value>().is_err());
    assert!(restored.build::<Value>().is_err());
    assert_roundtrip(optional, json!({}));
}

#[test]
fn optional_files_do_not_hide_parse_or_read_errors() {
    let dir = tempfile::tempdir().unwrap();
    let malformed = dir.path().join("malformed.toml");
    let invalid_utf8 = dir.path().join("invalid-utf8.toml");
    std::fs::write(&malformed, "value = [").unwrap();
    std::fs::write(&invalid_utf8, [0xff]).unwrap();
    // Reading a directory also fails, even if the caller can access its metadata.
    for path in [malformed.as_path(), invalid_utf8.as_path(), dir.path()] {
        for builder in [ConfigBuilder::new().file(path), ConfigBuilder::new().file_optional(path)] {
            let restored = ConfigBuilder::from_state(builder.state());
            assert!(builder.build::<Value>().is_err(), "{}", path.display());
            assert!(restored.build::<Value>().is_err(), "{}", path.display());
        }
    }
}

#[test]
fn public_source_descriptions_can_be_constructed_directly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.toml");
    std::fs::write(&path, "base = 'configured'\nvalue = '${base}'").unwrap();
    let state = ConfigState {
        sources: vec![ConfigSource::File { path, required: true }],
        enable_vars: true,
    };
    assert_roundtrip(ConfigBuilder::from_state(state), json!({"base": "configured", "value": "configured"}));
}

#[cfg(feature = "http1")]
#[tokio::test]
async fn explicit_source_errors_reach_the_server_startup_caller() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.toml");
    let malformed = dir.path().join("malformed.toml");
    let wrong_type = dir.path().join("wrong-type.toml");
    std::fs::write(&malformed, "value = [").unwrap();
    std::fs::write(&wrong_type, "[server]\nhost = 'localhost'\nport = 'not-a-number'").unwrap();

    // Occupy the address so a regression that swallows config errors returns Bind instead of
    // hanging in a live server. The expected Config error must happen before binding.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    for app in [
        Gotcha::new().with_file_config(&missing),
        Gotcha::new().with_file_config(&malformed),
        Gotcha::new().with_optional_config(&malformed),
        Gotcha::new().with_optional_config(dir.path()),
        Gotcha::new().with_optional_config(&wrong_type),
    ] {
        let error = app.listen_on(addr).await.unwrap_err();
        assert!(matches!(error, GotchaError::Config(_)), "{error}");
    }

    // An absent optional source succeeds at configuration loading and reaches the bind step.
    let error = Gotcha::new().with_optional_config(&missing).listen_on(addr).await.unwrap_err();
    assert!(matches!(error, GotchaError::Bind { .. }), "{error}");

    // An already supplied config wins over accumulated sources, regardless of call order.
    for app in [
        Gotcha::new().config(Default::default()).with_file_config(&missing),
        Gotcha::new().with_file_config(&missing).config(Default::default()),
    ] {
        let error = app.listen_on(addr).await.unwrap_err();
        assert!(matches!(error, GotchaError::Bind { .. }), "{error}");
    }
}
