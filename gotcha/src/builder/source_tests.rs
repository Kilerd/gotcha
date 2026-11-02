//! The `with_*` methods add to the standard sources, whatever order they are called in.
//!
//! Configuration reads the process environment and working directory, so each test re-runs itself
//! in a child process that owns both.

use std::collections::BTreeMap;

use super::*;
use crate::config::tests::in_isolated_child;
use crate::config::{Config, ConfigSource, ServerConfig};
use crate::{GotchaApp, GotchaRouter};

type Layers = BTreeMap<String, String>;
type App = Gotcha<EmptyState, Layers>;

/// One key per layer, lower-case letters only: mofa's path variables do not match digits.
const KEYS: [&str; 7] = ["a", "b", "c", "d", "e", "f", "g"];

/// One environment layer per prefix: each sets the keys from its own position onwards.
const LAYERED_ENV: &[(&str, &str)] = &[
    ("GOTCHA_ACTIVE_PROFILE", "review"),
    ("GOTCHA_SOURCE_TOKEN", "resolved"),
    ("APP_E", "app-env"),
    ("APP_F", "app-env"),
    ("APP_G", "app-env"),
    ("GOTCHAX_F", "x-env"),
    ("GOTCHAX_G", "x-env"),
    ("GOTCHAY_G", "y-env"),
];

/// The winner of each key with only the standard sources.
const STANDARD: [&str; 7] = ["application", "profile", "profile", "profile", "app-env", "app-env", "app-env"];

/// The winner of each key with `first.toml`, `second.toml`, `GOTCHAX` and `GOTCHAY` added.
const ALL_ADDED: [&str; 7] = ["application", "profile", "first", "second", "app-env", "x-env", "y-env"];

/// Write one file per layer. Layer `n` sets key `n` and every key after it, so key `n` is won by
/// layer `n` exactly when it beats every layer below it: comparing all seven pins the whole order.
fn write_layered_files() {
    let layer = |name: &str, position: usize| KEYS[position - 1..].iter().map(|key| format!("{key} = '{name}'\n")).collect::<String>();
    std::fs::create_dir("configurations").unwrap();
    std::fs::write(
        "configurations/application.toml",
        layer("application", 1) + "reference = '${GOTCHA_SOURCE_TOKEN}/${g}'\n[server]\nport = 18080\n",
    )
    .unwrap();
    std::fs::write("configurations/application_review.toml", layer("profile", 2)).unwrap();
    std::fs::write("first.toml", layer("first", 3)).unwrap();
    std::fs::write("second.toml", layer("second", 4)).unwrap();
}

fn app() -> App {
    Gotcha::with_config::<Layers>()
}

fn sources(app: &App) -> Vec<ConfigSource> {
    match &app.config {
        Configuration::Deferred { sources, .. } => sources.state().sources,
        Configuration::Provided(_) | Configuration::Custom(_) => panic!("sources were replaced by a provided configuration or a loader"),
    }
}

/// Load `app`'s configuration the way startup does.
async fn load(mut app: App) -> GotchaResult<ConfigWrapper<Layers>> {
    app.config.resolve().await
}

fn file(path: &str, required: bool) -> ConfigSource {
    ConfigSource::File { path: path.into(), required }
}

fn env(prefix: &str) -> ConfigSource {
    ConfigSource::Env { prefix: prefix.into() }
}

/// The standard sources with `files` above the conventional files and `prefixes` above `APP`.
fn standard_with(files: Vec<ConfigSource>, prefixes: Vec<ConfigSource>) -> Vec<ConfigSource> {
    let mut sources = ConfigBuilder::standard().state().sources;
    let environment = sources.len() - 1;
    sources.splice(environment..environment, files);
    sources.extend(prefixes);
    sources
}

#[track_caller]
fn assert_winners(case: &str, config: ConfigWrapper<Layers>, winners: [&str; 7]) {
    let mut expected: Layers = KEYS.iter().zip(winners).map(|(key, winner)| (key.to_string(), winner.to_string())).collect();
    expected.insert("reference".into(), format!("resolved/{}", winners[6]));
    assert_eq!(config.app, expected, "{case}");
    assert_eq!(config.server.port, 18080, "{case}: application.toml beats the framework default");
    assert_eq!(config.server.host, ServerConfig::default().host, "{case}: the framework default fills the rest");
}

/// A trait application that keeps the default `config()`.
struct DefaultConfigApp;

impl GotchaApp for DefaultConfigApp {
    type State = ();
    type Config = Layers;

    fn routes(&self, router: GotchaRouter<GotchaContext<(), Layers>>) -> GotchaRouter<GotchaContext<(), Layers>> {
        router
    }

    async fn state(&self, _: &ConfigWrapper<Layers>) -> GotchaResult<()> {
        Ok(())
    }
}

#[tokio::test]
#[allow(deprecated)]
async fn added_sources_take_their_layer_whatever_the_call_order() {
    if !in_isolated_child("builder::source_tests::added_sources_take_their_layer_whatever_the_call_order", LAYERED_ENV) {
        return;
    }
    write_layered_files();

    let expected = standard_with(vec![file("first.toml", true), file("second.toml", true)], vec![env("GOTCHAX"), env("GOTCHAY")]);
    for (case, app) in [
        (
            "files, then environment",
            app()
                .with_file_config("first.toml")
                .with_file_config("second.toml")
                .with_env_config("GOTCHAX")
                .with_env_config("GOTCHAY"),
        ),
        (
            "environment wins over files added after it",
            app()
                .with_env_config("GOTCHAX")
                .with_env_config("GOTCHAY")
                .with_file_config("first.toml")
                .with_file_config("second.toml"),
        ),
        (
            "interleaved",
            app()
                .with_env_config("GOTCHAX")
                .with_file_config("first.toml")
                .with_env_config("GOTCHAY")
                .with_file_config("second.toml"),
        ),
        (
            "after with_default_config",
            app()
                .with_default_config()
                .with_env_config("GOTCHAX")
                .with_file_config("first.toml")
                .with_env_config("GOTCHAY")
                .with_file_config("second.toml"),
        ),
        (
            "default helpers in between and after",
            app()
                .with_default_env()
                .with_file_config("first.toml")
                .with_env_config("GOTCHAX")
                .with_default_files()
                .with_file_config("second.toml")
                .enable_variable_substitution()
                .with_env_config("GOTCHAY")
                .with_default_config(),
        ),
    ] {
        assert_eq!(sources(&app), expected, "{case}");
        assert_winners(case, load(app).await.unwrap(), ALL_ADDED);
    }
}

#[tokio::test]
async fn call_order_only_decides_within_a_layer() {
    if !in_isolated_child("builder::source_tests::call_order_only_decides_within_a_layer", LAYERED_ENV) {
        return;
    }
    write_layered_files();

    let app = app()
        .with_env_config("GOTCHAY")
        .with_optional_config("second.toml")
        .with_env_config("GOTCHAX")
        .with_optional_config("first.toml");
    assert_winners(
        "later files and prefixes win among their own kind",
        load(app).await.unwrap(),
        ["application", "profile", "first", "first", "app-env", "x-env", "x-env"],
    );
}

/// A single call used to replace the automatic sources with the framework defaults alone.
#[tokio::test]
async fn a_single_added_source_keeps_the_automatic_sources() {
    if !in_isolated_child("builder::source_tests::a_single_added_source_keeps_the_automatic_sources", LAYERED_ENV) {
        return;
    }
    write_layered_files();

    for (case, app, winners) in [
        (
            "with_env_config",
            app().with_env_config("GOTCHAX"),
            ["application", "profile", "profile", "profile", "app-env", "x-env", "x-env"],
        ),
        (
            "with_file_config",
            app().with_file_config("first.toml"),
            ["application", "profile", "first", "first", "app-env", "app-env", "app-env"],
        ),
        (
            "with_optional_config",
            app().with_optional_config("first.toml"),
            ["application", "profile", "first", "first", "app-env", "app-env", "app-env"],
        ),
        ("with_optional_config, missing", app().with_optional_config("missing.toml"), STANDARD),
    ] {
        assert_winners(case, load(app).await.unwrap(), winners);
    }
}

#[tokio::test]
async fn a_single_added_source_binds_the_configured_port() {
    if !in_isolated_child("builder::source_tests::a_single_added_source_binds_the_configured_port", &[]) {
        return;
    }
    // Occupy the configured port: reaching it fails with its address instead of serving.
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = occupied.local_addr().unwrap();
    std::fs::create_dir("configurations").unwrap();
    std::fs::write("configurations/application.toml", format!("[server]\nport = {}\n", address.port())).unwrap();

    // A regression would serve on the framework's default port; the timeout turns that into a failure.
    let run = Gotcha::new().with_env_config("GOTCHAX").run();
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), run)
        .await
        .expect("startup must stop at the occupied configured port")
        .unwrap_err();
    assert!(matches!(&error, GotchaError::Bind { addr, .. } if *addr == address.to_string()), "{error}");
}

#[tokio::test]
#[allow(deprecated)]
async fn default_helpers_and_substitution_are_the_standard_sources() {
    if !in_isolated_child("builder::source_tests::default_helpers_and_substitution_are_the_standard_sources", LAYERED_ENV) {
        return;
    }
    write_layered_files();

    let standard = ConfigBuilder::standard().state();
    for (case, app) in [
        ("no call", app()),
        ("with_default_config", app().with_default_config()),
        ("with_default_files", app().with_default_files()),
        ("with_default_env", app().with_default_env()),
        ("files, then env", app().with_default_files().with_default_env()),
        ("env, then files", app().with_default_env().with_default_files()),
        ("repeated", app().with_default_config().with_default_config()),
        ("enable_variable_substitution", app().enable_variable_substitution()),
    ] {
        assert_eq!(sources(&app), standard.sources, "{case}");
        assert_winners(case, load(app).await.unwrap(), STANDARD);
    }
}

#[tokio::test]
async fn every_default_path_loads_the_same_sources() {
    if !in_isolated_child("builder::source_tests::every_default_path_loads_the_same_sources", LAYERED_ENV) {
        return;
    }
    write_layered_files();

    assert_winners("builder", load(app()).await.unwrap(), STANDARD);
    assert_winners("trait", DefaultConfigApp.config().await.unwrap(), STANDARD);
    assert_winners("load_default", Config::load_default().unwrap(), STANDARD);
    assert_winners("standard", ConfigBuilder::standard().build().unwrap(), STANDARD);
}

#[tokio::test]
async fn repeated_sources_are_read_once_and_keep_their_place() {
    if !in_isolated_child("builder::source_tests::repeated_sources_are_read_once_and_keep_their_place", LAYERED_ENV) {
        return;
    }
    write_layered_files();

    let expected = standard_with(vec![file("first.toml", true), file("second.toml", true)], vec![env("GOTCHAX"), env("GOTCHAY")]);
    let repeated = app()
        .with_file_config("first.toml")
        .with_env_config("GOTCHAX")
        .with_file_config("second.toml")
        .with_env_config("GOTCHAY")
        .with_file_config("first.toml")
        .with_env_config("GOTCHAX");
    assert_eq!(sources(&repeated), expected);
    assert_winners("repeated", load(repeated).await.unwrap(), ALL_ADDED);

    // Naming a standard source keeps it in its layer: `APP` stays below added prefixes, and
    // application.toml stays below the profile file.
    let named = app()
        .with_env_config("GOTCHAX")
        .with_env_config("APP")
        .with_optional_config("configurations/application.toml");
    assert_eq!(sources(&named), standard_with(vec![], vec![env("GOTCHAX")]));
    assert_winners(
        "standard sources named again",
        load(named).await.unwrap(),
        ["application", "profile", "profile", "profile", "app-env", "x-env", "x-env"],
    );
}

/// With a profile active, another spelling of the base file must not lift it above the profile.
#[tokio::test]
async fn another_spelling_of_a_listed_path_keeps_its_place() {
    if !in_isolated_child("builder::source_tests::another_spelling_of_a_listed_path_keeps_its_place", LAYERED_ENV) {
        return;
    }
    write_layered_files();

    let standard = ConfigBuilder::standard().state().sources;
    let absolute = std::env::current_dir().unwrap().join("configurations/application.toml");
    for (case, app) in [
        ("leading ./", app().with_optional_config("./configurations/application.toml")),
        ("interior ./", app().with_optional_config("configurations/./application.toml")),
        ("repeated separator", app().with_optional_config("configurations//application.toml")),
        ("absolute", app().with_optional_config(&absolute)),
    ] {
        assert_eq!(sources(&app), standard, "{case}");
        assert_winners(case, load(app).await.unwrap(), STANDARD);
    }

    // The same goes for added files, and the first spelling stays in the list.
    let spelled = app().with_file_config("first.toml").with_optional_config("./first.toml");
    assert_eq!(sources(&spelled), standard_with(vec![file("first.toml", true)], vec![]));

    // `..` is not resolved, because that takes the file system, so it counts as a separate file.
    let parent = "configurations/../configurations/application.toml";
    let separate = app().with_optional_config(parent);
    assert_eq!(sources(&separate), standard_with(vec![file(parent, false)], vec![]));
    assert_winners(
        "a path through `..` is an added file",
        load(separate).await.unwrap(),
        ["application", "application", "application", "application", "app-env", "app-env", "app-env"],
    );
}

/// #83: the typed framework defaults let an environment value reach `u16` without any file.
#[tokio::test]
#[allow(deprecated)]
async fn environment_only_server_port_works_on_every_path() {
    if !in_isolated_child(
        "builder::source_tests::environment_only_server_port_works_on_every_path",
        &[("APP_SERVER__PORT", "9000"), ("GOTCHAX_SERVER__HOST", "0.0.0.0")],
    ) {
        return;
    }

    for (case, app) in [
        ("no call", app()),
        ("with_default_config", app().with_default_config()),
        ("with_default_files", app().with_default_files()),
        ("with_default_env", app().with_default_env()),
        ("enable_variable_substitution", app().enable_variable_substitution()),
        ("with_optional_config", app().with_optional_config("missing.toml")),
    ] {
        let config = load(app).await.unwrap();
        assert_eq!(config.server.port, 9000, "{case}");
        assert_eq!(config.server.host, ServerConfig::default().host, "{case}");
    }
    let config = load(app().with_env_config("GOTCHAX")).await.unwrap();
    assert_eq!((config.server.host.as_str(), config.server.port), ("0.0.0.0", 9000));

    let config = DefaultConfigApp.config().await.unwrap();
    assert_eq!(config.server.port, 9000, "trait");
    let config: ConfigWrapper<Layers> = Config::load_default().unwrap();
    assert_eq!(config.server.port, 9000, "load_default");
}

#[tokio::test]
#[allow(deprecated)]
async fn missing_required_files_still_fail_under_the_strict_policy() {
    if !in_isolated_child("builder::source_tests::missing_required_files_still_fail_under_the_strict_policy", &[]) {
        return;
    }

    for (case, mut app) in [
        ("alone", app().with_file_config("missing.toml")),
        ("after an environment source", app().with_env_config("GOTCHAX").with_file_config("missing.toml")),
        ("after with_default_config", app().with_default_config().with_file_config("missing.toml")),
        (
            "required after optional",
            app().with_optional_config("missing.toml").with_file_config("missing.toml"),
        ),
        (
            "optional after required",
            app().with_file_config("missing.toml").with_optional_config("missing.toml"),
        ),
        (
            "a conventional file named as required",
            app().with_file_config("configurations/application.toml"),
        ),
    ] {
        let error = app.config.resolve().await.err().unwrap_or_else(|| panic!("{case}: expected an error"));
        assert!(error.to_string().contains("required configuration file"), "{case}: {error}");

        // Startup applies the policy to this error exactly as before.
        assert!(ConfigErrorPolicy::<Layers>::Strict.apply(app.config.resolve().await).is_err(), "{case}");
        assert!(
            ConfigErrorPolicy::<Layers>::fallback_to_default().apply(app.config.resolve().await).is_ok(),
            "{case}"
        );
    }
}

#[tokio::test]
#[allow(deprecated)]
async fn invalid_conventional_files_fail_every_chain_that_includes_them() {
    if !in_isolated_child(
        "builder::source_tests::invalid_conventional_files_fail_every_chain_that_includes_them",
        &[("GOTCHA_ACTIVE_PROFILE", "review")],
    ) {
        return;
    }
    std::fs::create_dir("configurations").unwrap();

    for path in ["configurations/application.toml", "configurations/application_review.toml"] {
        std::fs::write(path, "value = [").unwrap();
        for app in [
            app().with_env_config("GOTCHAX"),
            app().with_optional_config("missing.toml"),
            app().enable_variable_substitution(),
        ] {
            assert!(matches!(load(app).await, Err(GotchaError::Config(_))), "{path}");
        }
        std::fs::remove_file(path).unwrap();
    }
}

/// Two valid files can conflict, and now the automatic sources take part in that.
#[tokio::test]
async fn a_key_with_different_types_in_two_layers_fails_loading() {
    if !in_isolated_child("builder::source_tests::a_key_with_different_types_in_two_layers_fails_loading", &[]) {
        return;
    }
    std::fs::create_dir("configurations").unwrap();
    std::fs::write("configurations/application.toml", "[db]\nurl = 'base'\n").unwrap();
    std::fs::write("local.toml", "db = 'local'\n").unwrap();

    let error = load(app().with_optional_config("local.toml")).await.err().expect("the layers conflict");
    assert!(error.to_string().contains("merge fail, path=$.db"), "{error}");
}

/// A fallback replaces the whole configuration, including the sources that did load.
#[tokio::test]
async fn a_fallback_after_an_automatic_source_fails_drops_added_files() {
    if !in_isolated_child("builder::source_tests::a_fallback_after_an_automatic_source_fails_drops_added_files", &[]) {
        return;
    }
    std::fs::create_dir("configurations").unwrap();
    std::fs::write("configurations/application.toml", "value = [").unwrap();
    std::fs::write("local.toml", "a = 'local'\n[server]\nport = 18080\n").unwrap();

    let chain = app().with_optional_config("local.toml");
    let config = ConfigErrorPolicy::<Layers>::fallback_to_default().apply(load(chain).await).unwrap();
    assert_eq!(config.app, Layers::new());
    assert_eq!(config.server.port, ServerConfig::default().port);

    // The recipe for the old chain lists only its own sources, so it keeps the added file.
    let recipe = app().load_config(|| async { Ok(ConfigBuilder::framework_defaults().file_optional("local.toml").enable_vars().build()?) });
    let config = load(recipe).await.unwrap();
    assert_eq!(config.app["a"], "local");
    assert_eq!(config.server.port, 18080);
}

#[tokio::test]
async fn build_config_starts_from_the_framework_defaults_and_keeps_insertion_order() {
    if !in_isolated_child(
        "builder::source_tests::build_config_starts_from_the_framework_defaults_and_keeps_insertion_order",
        LAYERED_ENV,
    ) {
        return;
    }
    write_layered_files();

    async fn built(configure: fn(ConfigBuilder) -> ConfigBuilder) -> ConfigWrapper<Layers> {
        load(app().build_config(configure).unwrap()).await.unwrap()
    }

    // Nothing but the framework defaults, even with files and the `APP` environment present.
    let config = built(|builder| builder).await;
    assert_eq!(config.app, Layers::new());
    assert_eq!(config.server.port, ServerConfig::default().port);

    // No substitution until the closure asks for it.
    let config = built(|builder| builder.file("configurations/application.toml")).await;
    assert_eq!(config.app["reference"], "${GOTCHA_SOURCE_TOKEN}/${g}");
    assert_eq!(config.app["g"], "application", "no profile file and no `APP` environment");

    // A later source wins, whatever its kind.
    let config = built(|builder| builder.env("GOTCHAX").file("first.toml")).await;
    assert_eq!(config.app["g"], "first");

    assert_winners("standard_sources", built(|builder| builder.standard_sources()).await, STANDARD);
    assert_winners(
        "a file after standard_sources overrides the environment",
        built(|builder| builder.standard_sources().file("first.toml")).await,
        ["application", "profile", "first", "first", "first", "first", "first"],
    );

    // Sources already listed keep their place, so these files land above `APP`.
    let files_win = ["application", "profile", "profile", "profile", "profile", "profile", "profile"];
    assert_winners(
        "env, then standard_sources",
        built(|builder| builder.env("APP").standard_sources()).await,
        files_win,
    );

    // The recipes for the old `with_default_files()` and `with_default_env()`.
    assert_winners(
        "conventional files only",
        built(|builder| builder.file_optional("configurations/application.toml").active_profile_file().enable_vars()).await,
        files_win,
    );
    let config = built(|builder| builder.env("APP").enable_vars()).await;
    assert_eq!(
        config.app,
        Layers::from_iter(["e", "f", "g"].map(|key| (key.to_string(), "app-env".to_string())))
    );
    assert_eq!(config.server.port, ServerConfig::default().port, "no conventional file");

    // Application defaults stay beneath the standard sources.
    #[derive(Serialize)]
    struct Defaults {
        only_default: String,
        a: String,
    }
    let config = built(|builder| {
        builder
            .defaults(Defaults {
                only_default: "default".into(),
                a: "default".into(),
            })
            .expect("defaults serialize")
            .standard_sources()
    })
    .await;
    assert_eq!(config.app["only_default"], "default");
    assert_eq!(config.app["a"], "application");
    assert_eq!(config.server.port, 18080);
}
