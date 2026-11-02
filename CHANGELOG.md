# Changelog

All notable changes to Gotcha are documented in this file, in the format of [Keep a
Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.0] - 2026-11-02

`gotcha`, `gotcha_core` and `gotcha_macro` are released together. To upgrade from 0.4.1, read [the
migration guide](MIGRATION.md#04--05).

### Highlights

- **OpenAPI 3.2, served on request.** Documents are now OpenAPI 3.2.0, and the `openapi` feature no
  longer mounts endpoints by itself: call `.with_openapi()` for `/openapi.json` and `/scalar`.
  ([#115](https://github.com/Kilerd/gotcha/pull/115),
  [#120](https://github.com/Kilerd/gotcha/pull/120))
- **One startup and shutdown path.** Builder `run()` now honours `config.server`, configuration
  errors stop startup, and Ctrl-C or SIGTERM drains HTTP and tasks, each within 30 seconds by
  default. ([#111](https://github.com/Kilerd/gotcha/pull/111),
  [#112](https://github.com/Kilerd/gotcha/pull/112),
  [#136](https://github.com/Kilerd/gotcha/pull/136))
- **JSON errors.** Extractors reject with a configurable JSON body, `Valid` reports `fields` under
  serde's names, `GotchaError` hides its message, and documents list rejections.
  ([#135](https://github.com/Kilerd/gotcha/pull/135),
  [#141](https://github.com/Kilerd/gotcha/pull/141),
  [#146](https://github.com/Kilerd/gotcha/pull/146),
  [#147](https://github.com/Kilerd/gotcha/pull/147))
- **`Schematic` follows serde.** Field presence, enum shapes, renames, `from`/`into` and
  `deny_unknown_fields` now track serde, so regenerate clients.
  ([#137](https://github.com/Kilerd/gotcha/pull/137),
  [#173](https://github.com/Kilerd/gotcha/pull/173),
  [#175](https://github.com/Kilerd/gotcha/pull/175),
  [#180](https://github.com/Kilerd/gotcha/pull/180),
  [#196](https://github.com/Kilerd/gotcha/pull/196),
  [#200](https://github.com/Kilerd/gotcha/pull/200),
  [#202](https://github.com/Kilerd/gotcha/pull/202),
  [#203](https://github.com/Kilerd/gotcha/pull/203),
  [#205](https://github.com/Kilerd/gotcha/pull/205))
- **Additive configuration sources.** `with_*` methods add to `configurations/application.toml`, the
  profile file and `APP_*` in a fixed precedence, instead of silently dropping them.
  ([#99](https://github.com/Kilerd/gotcha/pull/99),
  [#204](https://github.com/Kilerd/gotcha/pull/204))
- **`GotchaApp` is deprecated.** The `Gotcha` builder covers every trait method. `Message` and
  `Messager` left the crate; `scheduler.spawn` runs background work under the shutdown drain.
  ([#191](https://github.com/Kilerd/gotcha/pull/191),
  [#192](https://github.com/Kilerd/gotcha/pull/192),
  [#195](https://github.com/Kilerd/gotcha/pull/195),
  [#197](https://github.com/Kilerd/gotcha/pull/197),
  [#198](https://github.com/Kilerd/gotcha/pull/198))

### Breaking

Everything under Removed is breaking too. Generated documents change in many of these entries, so
regenerate clients built from them.

**Startup and shutdown**

- Builder `run()` binds the address in `config.server` unless `listen`, `.host` or `.port` say
  otherwise, and configuration errors stop startup unless a fallback is set.
  [Guide](MIGRATION.md#04--05-shared-startup) ([#111](https://github.com/Kilerd/gotcha/pull/111))
- Applications now handle Ctrl-C and SIGTERM and drain HTTP for up to 30 seconds. `GotchaContext`
  can no longer be built with a struct literal.
  [Guide](MIGRATION.md#04--05-bound-servers-and-shutdown-lifecycle)
  ([#136](https://github.com/Kilerd/gotcha/pull/136))
- Registering a task no longer starts it: `cron` and `interval` take `&mut self`, and a standalone
  scheduler needs `start()`, keeping the returned owner.
  [Guide](MIGRATION.md#04--05-owned-scheduled-tasks)
  ([#112](https://github.com/Kilerd/gotcha/pull/112))
- An invalid cron expression or zero interval now fails `bind()` and `run()` instead of being logged
  and dropped. [Guide](MIGRATION.md#04--05-invalid-task-registrations-fail-startup)
  ([#166](https://github.com/Kilerd/gotcha/pull/166))

**Configuration**

- `with_env_config`, `with_file_config`, `with_optional_config` and the default helpers add to the
  standard sources, so these applications start reading the conventional files and `APP_*`.
  [Guide](MIGRATION.md#04--05-additive-configuration-sources)
  ([#204](https://github.com/Kilerd/gotcha/pull/204))
- `ConfigState` holds an ordered `sources` list instead of `file_paths` and `env_prefixes`, and a
  failing explicit source stops builder startup.
  [Guide](MIGRATION.md#04--05-ordered-configuration-sources)
  ([#99](https://github.com/Kilerd/gotcha/pull/99))
- `Config::load_default` returns `ConfigResult<T>` instead of falling back to `T::default()`.
  [Guide](MIGRATION.md#04--05-default-configuration-sources)
  ([#133](https://github.com/Kilerd/gotcha/pull/133))
- `GotchaContext::config` is now an `Arc<ConfigWrapper<C>>`. Reads are unchanged; writing through
  the field or serializing it needs an edit. [Guide](MIGRATION.md#04--05-shared-configuration)
  ([#162](https://github.com/Kilerd/gotcha/pull/162))
- Generic callers of `GotchaApp::config()` or `run()` must state `Self::Config: DeserializeOwned`.
  Bounds on stored state and configuration are otherwise relaxed.
  [Guide](MIGRATION.md#04--05-runtime-type-bounds)
  ([#110](https://github.com/Kilerd/gotcha/pull/110))

**Errors and validation**

- `gotcha::{Json, Path, Query, Form, Multipart, TypedHeader}` are now Gotcha's wrappers. Rejections
  are JSON, and `Result<Json<T>, JsonRejection>` becomes `Result<Json<T>, Rejected<JsonRejection>>`.
  [Guide](MIGRATION.md#04--05-json-error-bodies-and-extractors)
  ([#135](https://github.com/Kilerd/gotcha/pull/135))
- A `GotchaError` response now always says `{"error": "internal server error"}`; the full error goes
  to `tracing::error!` only. [Guide](MIGRATION.md#04--05-json-error-bodies-and-extractors)
  ([#135](https://github.com/Kilerd/gotcha/pull/135))
- A `Valid` failure is still `422`, but its body is `ErrorBody` with a `fields` map instead of
  validator's `ValidationErrors`. [Guide](MIGRATION.md#04--05-validation-error-bodies)
  ([#141](https://github.com/Kilerd/gotcha/pull/141))
- `Valid<Json<T>>` requires `T: Schematic`, and error paths use serde's names (`firstName`, not
  `first_name`). A manual `impl Schematic for Box<Local>` now conflicts.
  [Guide](MIGRATION.md#04--05-schematic-in-every-build-and-wire-name-validation-errors)
  ([#147](https://github.com/Kilerd/gotcha/pull/147))
- `ValidRejection` and `ParamRejection` are `#[non_exhaustive]`, so a `match` on them needs a
  wildcard arm. [Guide](MIGRATION.md#04--05-non-exhaustive-extractor-enums)
  ([#150](https://github.com/Kilerd/gotcha/pull/150))

**Routing and middleware**

- `route` takes Gotcha's `MethodRouter`, which `gotcha::get`, `post` and the rest now return. For an
  Axum method router, use `route_raw`. [Guide](MIGRATION.md#04--05-documented-method-routers)
  ([#105](https://github.com/Kilerd/gotcha/pull/105))
- `Gotcha::nest` and `merge` take a `GotchaRouter`, not another `Gotcha`. Move child state,
  configuration and tasks to the top-level application.
  [Guide](MIGRATION.md#04--05-application-and-route-composition)
  ([#106](https://github.com/Kilerd/gotcha/pull/106))
- `with_cors()` covers the whole application, including documentation, `/metrics` and later routes,
  and answers preflights before any layer inside it.
  [Guide](MIGRATION.md#04--05-explicit-openapi-endpoints)
  ([#148](https://github.com/Kilerd/gotcha/pull/148))

**OpenAPI and `#[api]`**

- The `openapi` feature no longer serves documentation. Add `.with_openapi()` for `/openapi.json`
  and `/scalar`, or `.openapi_endpoints(..)` for custom paths.
  [Guide](MIGRATION.md#04--05-explicit-openapi-endpoints)
  ([#115](https://github.com/Kilerd/gotcha/pull/115))
- Documents are OpenAPI 3.2.0 on `oas` 0.3: tooling must support 3.2, custom `oas` code changes, and
  Redoc is off by default because it rejects 3.2. [Guide](MIGRATION.md#04--05-openapi-32)
  ([#120](https://github.com/Kilerd/gotcha/pull/120))
- `Schematic` no longer implies `Responsible`: return `Json<T>`, or implement `Responsible` for
  custom types. Text and binary responses are no longer described as JSON.
  [Guide](MIGRATION.md#04--05-http-response-contracts)
  ([#107](https://github.com/Kilerd/gotcha/pull/107))
- Operations document the rejections of Gotcha's extractors by default, so documents change without
  code changes. Opt out with `.document_extractor_errors(false)`.
  [Guide](MIGRATION.md#04--05-extractor-rejection-responses-in-openapi)
  ([#146](https://github.com/Kilerd/gotcha/pull/146))
- The default `info` block is now `title: "API"`, `version: "0.1.0"` and no description. Set your
  own with `.openapi_info(..)`. [Guide](MIGRATION.md#04--05-neutral-openapi-info-defaults)
  ([#140](https://github.com/Kilerd/gotcha/pull/140))
- Schemas are identified by their full Rust type. Colliding names become `Envelope_String` or
  `ScreensSendResult`; pin public names with `#[schematic(name = "..")]`.
  [Guide](MIGRATION.md#04--05-schema-identity-and-component-names)
  ([#102](https://github.com/Kilerd/gotcha/pull/102))
- Low-level `Operable` fields and `ParamConstructor` hold function pointers instead of `Lazy` and
  boxed closures. `#[api]` users are unaffected.
  [Guide](MIGRATION.md#04--05-static-openapi-descriptors)
  ([#113](https://github.com/Kilerd/gotcha/pull/113))

**`Schematic` derive**

- Presence follows serde: `deserialize_with` or `with` without `default` makes a field required, and
  `skip_serializing_if = "Option::is_none"` drops the null branch.
  [Guide](MIGRATION.md#04--05-serde-driven-field-presence)
  ([#137](https://github.com/Kilerd/gotcha/pull/137))
- `#[serde(untagged)]` enums are documented with `anyOf` instead of `oneOf`.
  [Guide](MIGRATION.md#04--05-untagged-enums-use-anyof)
  ([#180](https://github.com/Kilerd/gotcha/pull/180))
- `#[serde(skip)]` variants leave the document, and an `#[serde(untagged)]` variant moves a tagged
  enum to `anyOf` with no `discriminator`.
  [Guide](MIGRATION.md#04--05-skipped-and-untagged-variants)
  ([#200](https://github.com/Kilerd/gotcha/pull/200))
- A type converting through one proxy in both directions is documented as that proxy, which must
  implement `Schematic` or the derive stops compiling.
  [Guide](MIGRATION.md#04--05-from-and-into-proxies-document-the-type)
  ([#203](https://github.com/Kilerd/gotcha/pull/203))
- `#[serde(deny_unknown_fields)]` adds `additionalProperties: false`, where serde rejects unknown
  keys. [Guide](MIGRATION.md#04--05-deny_unknown_fields-closes-schemas)
  ([#205](https://github.com/Kilerd/gotcha/pull/205))
- `#[schematic(..)]` on a variant accepts only `title` and `description`, other keys are compile
  errors, and variant doc comments become descriptions.
  [Guide](MIGRATION.md#04--05-variant-level-schematic-attributes)
  ([#182](https://github.com/Kilerd/gotcha/pull/182))

**Cargo features**

- With `default-features = false`, `gotcha` and `gotcha_core` implement `Schematic` for `uuid`,
  `chrono`, `bigdecimal` and `rust_decimal` only if you list those features.
  [Guide](MIGRATION.md#04--05-data-type-schematic-impls-are-features)
  ([#170](https://github.com/Kilerd/gotcha/pull/170))

### Added

- Builder startup hooks from the trait: `from_state_fn`, `load_config`, `logger`,
  `with_default_logger` and `try_tasks`. ([#191](https://github.com/Kilerd/gotcha/pull/191))
- `Gotcha::into_router`, `try_finish_router` and `extractor_configs`, plus `fallback_service` and
  `method_route` on the builder. ([#192](https://github.com/Kilerd/gotcha/pull/192))
- `Gotcha::from_state` and `from_context` for state and configuration that are already initialized.
  ([#110](https://github.com/Kilerd/gotcha/pull/110))
- `bind`, `bind_on` and `BoundServer`, plus `.shutdown_signal`, `.http_shutdown_timeout`,
  `.on_shutdown` and `context.shutdown()`. ([#136](https://github.com/Kilerd/gotcha/pull/136),
  [#163](https://github.com/Kilerd/gotcha/pull/163),
  [#171](https://github.com/Kilerd/gotcha/pull/171),
  [#172](https://github.com/Kilerd/gotcha/pull/172))
- `scheduler.spawn(name, |ctx| ..)` runs any future under the shutdown drain, bounded by
  `.task_shutdown_timeout`; `RunningTasks` owns a standalone scheduler's work.
  ([#112](https://github.com/Kilerd/gotcha/pull/112),
  [#165](https://github.com/Kilerd/gotcha/pull/165),
  [#197](https://github.com/Kilerd/gotcha/pull/197))
- `.app_layer`, `.finish_router`, `route_layer`, `nest_service` and `route_raw` for middleware scope
  and mounting services. ([#105](https://github.com/Kilerd/gotcha/pull/105),
  [#115](https://github.com/Kilerd/gotcha/pull/115),
  [#132](https://github.com/Kilerd/gotcha/pull/132))
- `#[api]` declarations: `responses(..)`, `drop_default`, `errors(..)`, `skip_response`, `item` and
  `request_body(..)`, plus `gotcha::openapi::Binary` for upload forms.
  ([#107](https://github.com/Kilerd/gotcha/pull/107),
  [#109](https://github.com/Kilerd/gotcha/pull/109),
  [#130](https://github.com/Kilerd/gotcha/pull/130),
  [#142](https://github.com/Kilerd/gotcha/pull/142),
  [#157](https://github.com/Kilerd/gotcha/pull/157))
- `WithStatus<T, STATUS>` for a fixed status in the return type, and helpers in `gotcha::response`
  for custom contracts. ([#107](https://github.com/Kilerd/gotcha/pull/107))
- `HttpError`, `ErrorBody`, `Rejected<R>` and per-provider configuration:
  `.extractor_config(JsonConfig::default().error_handler(..).statuses(..))`.
  ([#135](https://github.com/Kilerd/gotcha/pull/135),
  [#146](https://github.com/Kilerd/gotcha/pull/146),
  [#161](https://github.com/Kilerd/gotcha/pull/161))
- Extractor providers: `RejectionProvider` and `kind::<P>()` give an extractor outside Gotcha the
  same configuration and documentation. [Keys](MIGRATION.md#04--05-extractor-provider-keys),
  [built-ins](MIGRATION.md#04--05-built-in-extractors-report-provider-keys)
  ([#186](https://github.com/Kilerd/gotcha/pull/186),
  [#187](https://github.com/Kilerd/gotcha/pull/187),
  [#188](https://github.com/Kilerd/gotcha/pull/188),
  [#206](https://github.com/Kilerd/gotcha/pull/206))
- `Gotcha::metrics(MetricsEndpoint::default())` and `GotchaApp::metrics_endpoint` install Prometheus
  metrics at `/metrics`; the `prometheus` feature alone serves nothing.
  ([#143](https://github.com/Kilerd/gotcha/pull/143))
- `.openapi_info(..)`, `gotcha::cargo_info!()`, and `into_openapi()` to export a document without
  HTTP. ([#115](https://github.com/Kilerd/gotcha/pull/115),
  [#140](https://github.com/Kilerd/gotcha/pull/140))
- `#[schematic(name = "..")]` to pin a component name and `#[schematic(as = "..")]` to describe a
  field as another type. ([#102](https://github.com/Kilerd/gotcha/pull/102),
  [#147](https://github.com/Kilerd/gotcha/pull/147))
- `Schematic` for `BTreeMap`, `BTreeSet`, `VecDeque`, `[T; N]`, `Box`, `Arc`, `Rc`, `Cow`, `str`,
  `char`, `i128`, `u128`, `NonZero*`, IP addresses and more chrono types.
  ([#126](https://github.com/Kilerd/gotcha/pull/126),
  [#147](https://github.com/Kilerd/gotcha/pull/147))
- `Path<(A, B, C, ..)>` tuples are documented up to sixteen segments.
  ([#127](https://github.com/Kilerd/gotcha/pull/127))
- `Option<Header<T>>` and `Option<Cookie<T>>` extract an absent value as `None`.
  ([#149](https://github.com/Kilerd/gotcha/pull/149))
- `#[derive(gotcha::Schematic)]` without a direct `gotcha_core` dependency, a Cargo-renamed `gotcha`
  in the derive and attribute macros, and `gotcha::Validate` with `gotcha::validator`.
  [Guide](MIGRATION.md#04--05-derive-dependency-paths)
  ([#117](https://github.com/Kilerd/gotcha/pull/117),
  [#156](https://github.com/Kilerd/gotcha/pull/156))
- `ConfigErrorPolicy`, and on `ConfigBuilder`: `standard()`, `standard_sources()`,
  `active_profile_file()`, `framework_defaults()` and `defaults(value)`.
  ([#111](https://github.com/Kilerd/gotcha/pull/111),
  [#129](https://github.com/Kilerd/gotcha/pull/129),
  [#204](https://github.com/Kilerd/gotcha/pull/204))

### Changed

- All three crates declare `rust-version = "1.89"`, which the default features need through `uuid`,
  and CI checks them with Rust 1.89. [Guide](MIGRATION.md#04--05-the-macro-crate-needs-rust-188)
- `gotcha_macro` moves to syn 2 and darling 0.23 and drops syn 1 and the unmaintained
  `proc-macro-error` (RUSTSEC-2024-0370).
  [Guide](MIGRATION.md#04--05-the-macro-crate-needs-rust-188)
  ([#189](https://github.com/Kilerd/gotcha/pull/189),
  [#190](https://github.com/Kilerd/gotcha/pull/190))
- Repeated `.openapi(..)` calls append instead of replacing, and `nest` and `merge` keep child
  callbacks, so previously discarded callbacks now run.
  [Guide](MIGRATION.md#04--05-openapi-transform-composition)
  ([#104](https://github.com/Kilerd/gotcha/pull/104))
- A slow scheduled execution no longer causes back-to-back catch-up runs, for interval and cron
  tasks alike. [Guide](MIGRATION.md#04--05-owned-scheduled-tasks)
  ([#164](https://github.com/Kilerd/gotcha/pull/164))
- `TaskScheduler`, `spawn` and `interval` no longer need the `task` feature, which now only adds
  `cron`. ([#197](https://github.com/Kilerd/gotcha/pull/197))
- `Schematic`, its derive and `WireField` are available without `openapi`, and `gotcha_core` is a
  required dependency of `gotcha`.
  [Guide](MIGRATION.md#04--05-schematic-in-every-build-and-wire-name-validation-errors)
  ([#147](https://github.com/Kilerd/gotcha/pull/147))
- `/openapi.json` is serialized once at assembly instead of on every request; the bytes are
  unchanged. ([#168](https://github.com/Kilerd/gotcha/pull/168))

### Deprecated

- The `GotchaApp` trait, in favour of the `Gotcha` builder. It keeps working with a warning until a
  later release removes it; a crate that denies warnings needs `#[allow(deprecated)]`.
  [Guide](MIGRATION.md#04--05-moving-from-gotchaapp-to-the-builder)
  ([#193](https://github.com/Kilerd/gotcha/pull/193),
  [#194](https://github.com/Kilerd/gotcha/pull/194),
  [#195](https://github.com/Kilerd/gotcha/pull/195))
- `with_default_config`, `with_default_files`, `with_default_env` and
  `enable_variable_substitution`, which now do nothing: delete the call.
  [Guide](MIGRATION.md#04--05-additive-configuration-sources)
  ([#204](https://github.com/Kilerd/gotcha/pull/204))

### Removed

- `gotcha::Message`, `gotcha::Messager` and the `gotcha::message` module. The guide has a module to
  copy into your crate. [Guide](MIGRATION.md#04--05-message-and-messager-moved-out-of-gotcha)
  ([#114](https://github.com/Kilerd/gotcha/pull/114),
  [#198](https://github.com/Kilerd/gotcha/pull/198))
- `Gotcha::quick_start`, `quick_server!`, `handler!`, `gotcha::Lazy`, `GotchaConfig`,
  `gotcha::prelude::Result` and two `gotcha::openapi` path helpers.
  [Guide](MIGRATION.md#04--05-removed-unused-public-items)
  ([#169](https://github.com/Kilerd/gotcha/pull/169))
- The `http1` feature: HTTP/1 is always enabled, so remove it from explicit feature lists.
  [Guide](MIGRATION.md#04--05-http1-is-always-enabled)
  ([#118](https://github.com/Kilerd/gotcha/pull/118))
- `ErrorResponsible` and `Schematic::empty_body`.
  [Guide](MIGRATION.md#04--05-http-response-contracts)
  ([#107](https://github.com/Kilerd/gotcha/pull/107))

### Fixed

- Documents now match serde for unit variants, multi-field tuple variants, struct-variant field
  attributes, `transparent` structs and flattened maps.
  [Guide](MIGRATION.md#04--05-derive-documents-serdes-wire-shapes)
  ([#173](https://github.com/Kilerd/gotcha/pull/173),
  [#174](https://github.com/Kilerd/gotcha/pull/174),
  [#175](https://github.com/Kilerd/gotcha/pull/175),
  [#176](https://github.com/Kilerd/gotcha/pull/176),
  [#177](https://github.com/Kilerd/gotcha/pull/177),
  [#178](https://github.com/Kilerd/gotcha/pull/178),
  [#179](https://github.com/Kilerd/gotcha/pull/179))
- Field and variant names follow serde's rules for raw identifiers and `rename_all`, and the list
  forms of `rename` are read. [Guide](MIGRATION.md#04--05-names-serde-writes-and-reads-differently)
  ([#196](https://github.com/Kilerd/gotcha/pull/196),
  [#202](https://github.com/Kilerd/gotcha/pull/202))
- An internally tagged struct documents its tag, and a `#[serde(skip)]` tuple-variant field no
  longer takes a position. ([#199](https://github.com/Kilerd/gotcha/pull/199),
  [#201](https://github.com/Kilerd/gotcha/pull/201))
- `#[serde(flatten)]` on an `Option<T>` field no longer disappears from the document and from
  `Query` parameters. ([#160](https://github.com/Kilerd/gotcha/pull/160))
- `#[validate(regex(path = *RE))]`, `length(max = CONST)` and `#[doc = include_str!(..)]` no longer
  abort macro expansion. ([#155](https://github.com/Kilerd/gotcha/pull/155))
- `#[validate(length(..))]` documents items or properties keywords on sets, deques, maps and wrapped
  arrays, not string keywords.
  [Guide](MIGRATION.md#04--05-validation-length-keywords-follow-the-schema-type)
  ([#159](https://github.com/Kilerd/gotcha/pull/159))
- `#[api(skip)]` is parsed structurally: `#[api(skipper)]` used to skip the extractor silently and
  is now a compile error. [Guide](MIGRATION.md#04--05-http-response-contracts)
  ([#134](https://github.com/Kilerd/gotcha/pull/134))
- Two operations can no longer emit the same `operationId`: later duplicates get `_2`, `_3` and so
  on. [Guide](MIGRATION.md#04--05-duplicate-operation-ids-are-suffixed)
  ([#167](https://github.com/Kilerd/gotcha/pull/167))
- Nesting `/hello` under `/api` documents `/api/hello`, not `/api//hello`.
  [Guide](MIGRATION.md#04--05-nested-openapi-paths)
  ([#103](https://github.com/Kilerd/gotcha/pull/103))
- `Option<Json<T>>` and `Option<Multipart>` bodies are documented as optional, and path parameters
  are always required. ([#131](https://github.com/Kilerd/gotcha/pull/131))
- Documents declare the top-level `tags` list for `#[api(group = "..")]` routes.
  ([#125](https://github.com/Kilerd/gotcha/pull/125))
- A header or cookie parameter with `REQUIRED = false` and the default `missing()` is documented as
  required, matching its runtime `400`. ([#149](https://github.com/Kilerd/gotcha/pull/149))
- `APP_SERVER__PORT=9000` loads with no configuration file instead of failing on the string type.
  [Guide](MIGRATION.md#04--05-typed-configuration-defaults)
  ([#129](https://github.com/Kilerd/gotcha/pull/129))
- `${VAR}` and path placeholders resolve for applications using `with_default_config`,
  `with_default_files` or `with_default_env`.
  [Guide](MIGRATION.md#04--05-default-configuration-sources)
  ([#133](https://github.com/Kilerd/gotcha/pull/133))
- `json_response!({"ok": true})` compiles, and callers no longer need direct `axum` or `serde_json`
  dependencies. ([#135](https://github.com/Kilerd/gotcha/pull/135))

CI, test, documentation and refactoring changes with no visible effect are omitted:
[#101](https://github.com/Kilerd/gotcha/pull/101),
[#145](https://github.com/Kilerd/gotcha/pull/145),
[#151](https://github.com/Kilerd/gotcha/pull/151),
[#152](https://github.com/Kilerd/gotcha/pull/152),
[#153](https://github.com/Kilerd/gotcha/pull/153),
[#154](https://github.com/Kilerd/gotcha/pull/154),
[#158](https://github.com/Kilerd/gotcha/pull/158),
[#181](https://github.com/Kilerd/gotcha/pull/181),
[#183](https://github.com/Kilerd/gotcha/pull/183),
[#184](https://github.com/Kilerd/gotcha/pull/184),
[#185](https://github.com/Kilerd/gotcha/pull/185).

## Earlier versions

Changes before 0.5.0 are in [MIGRATION.md](MIGRATION.md) and the git history.

[0.5.0]: https://github.com/Kilerd/gotcha/compare/v0.4.1...v0.5.0
