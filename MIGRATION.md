# Migration Guide

- [Unreleased: deny_unknown_fields closes schemas](#unreleased-deny_unknown_fields-closes-schemas)
- [Unreleased: from and into proxies document the type](#unreleased-from-and-into-proxies-document-the-type)
- [Unreleased: moving from GotchaApp to the builder](#unreleased-moving-from-gotchaapp-to-the-builder)
- [Unreleased: extractor provider keys](#unreleased-extractor-provider-keys)
- [Unreleased: skipped and untagged variants](#unreleased-skipped-and-untagged-variants)
- [Unreleased: names serde writes and reads differently](#unreleased-names-serde-writes-and-reads-differently)
- [Unreleased: the macro crate needs Rust 1.88](#unreleased-the-macro-crate-needs-rust-188)
- [Unreleased: untagged enums use anyOf](#unreleased-untagged-enums-use-anyof)
- [Unreleased: variant-level schematic attributes](#unreleased-variant-level-schematic-attributes)
- [Unreleased: derive documents serde's wire shapes](#unreleased-derive-documents-serdes-wire-shapes)
- [Unreleased: invalid task registrations fail startup](#unreleased-invalid-task-registrations-fail-startup)
- [Unreleased: non-exhaustive extractor enums](#unreleased-non-exhaustive-extractor-enums)
- [Unreleased: shared configuration](#unreleased-shared-configuration)
- [Unreleased: removed unused public items](#unreleased-removed-unused-public-items)
- [Unreleased: duplicate operation ids are suffixed](#unreleased-duplicate-operation-ids-are-suffixed)
- [Unreleased: data-type Schematic impls are features](#unreleased-data-type-schematic-impls-are-features)
- [Unreleased: validation `length` keywords follow the schema type](#unreleased-validation-length-keywords-follow-the-schema-type)
- [Unreleased: extractor rejection responses in OpenAPI](#unreleased-extractor-rejection-responses-in-openapi)
- [Unreleased: neutral OpenAPI info defaults](#unreleased-neutral-openapi-info-defaults)
- [Unreleased: serde-driven field presence](#unreleased-serde-driven-field-presence)
- [Unreleased: validation error bodies](#unreleased-validation-error-bodies)
- [Unreleased: Schematic in every build and wire-name validation errors](#unreleased-schematic-in-every-build-and-wire-name-validation-errors)
- [Unreleased: JSON error bodies and extractors](#unreleased-json-error-bodies-and-extractors)
- [Unreleased: OpenAPI 3.2](#unreleased-openapi-32)
- [Unreleased: HTTP/1 is always enabled](#unreleased-http1-is-always-enabled)
- [Unreleased: derive dependency paths](#unreleased-derive-dependency-paths)
- [Unreleased: explicit OpenAPI endpoints](#unreleased-explicit-openapi-endpoints)
- [Unreleased: message task handles](#unreleased-message-task-handles)
- [Unreleased: static OpenAPI descriptors](#unreleased-static-openapi-descriptors)
- [Unreleased: owned scheduled tasks](#unreleased-owned-scheduled-tasks)
- [Unreleased: shared startup](#unreleased-shared-startup)
- [Unreleased: runtime type bounds](#unreleased-runtime-type-bounds)
- [Unreleased: bound servers and shutdown lifecycle](#unreleased-bound-servers-and-shutdown-lifecycle)
- [Unreleased: HTTP response contracts](#unreleased-http-response-contracts)
- [Unreleased: application and route composition](#unreleased-application-and-route-composition)
- [Unreleased: documented method routers](#unreleased-documented-method-routers)
- [Unreleased: OpenAPI transform composition](#unreleased-openapi-transform-composition)
- [Unreleased: nested OpenAPI paths](#unreleased-nested-openapi-paths)
- [Unreleased: schema identity and component names](#unreleased-schema-identity-and-component-names)
- [Unreleased: ordered configuration sources](#unreleased-ordered-configuration-sources)
- [Unreleased: typed configuration defaults](#unreleased-typed-configuration-defaults)
- [Unreleased: default configuration sources](#unreleased-default-configuration-sources)
- [0.3 → 0.4](#03--04) — **every application must edit its route paths and configuration file**
- [0.2 → 0.3: API simplification](#02--03-api-simplification)

---

# Unreleased: deny_unknown_fields closes schemas

`#[serde(deny_unknown_fields)]` now adds `additionalProperties: false` to the schema, where serde
rejects unknown keys:

- a struct;
- the body of a struct variant, in every tag form;
- in an adjacently tagged enum, also the object holding the tag and the content.

Documents become stricter and generated clients may change (for example, a TypeScript type loses
its index signature), so regenerate them. A key the document now refuses but serde reads:

- an `alias`, which the document does not list;
- `{"t": "U", "c": null}` for an adjacently tagged unit variant, which serde also reads as
  `{"t": "U"}`.

These stay as they were:

- A struct or variant with a `#[serde(flatten)]` field, because serde documents the combination as
  not supported. A flattened struct, or an externally or adjacently tagged enum, reads valid values
  and rejects unknown keys. A flattened internally tagged or untagged enum rejects every value. A
  flattened map reads an empty map and rejects entries.
- A struct or variant with a `skip_serializing` field, and a struct with a `skip_deserializing`
  field: the schema leaves out a key serde reads or writes, so a closed schema would reject it.
- A struct or variant with a field serde writes and reads under different names, through `rename`,
  `rename_all` or `rename_all_fields`: the schema names one of them, so a closed schema would reject
  the other.
- A type with `from`, `try_from` or `into`: serde writes or reads the other type, whose keys this
  type's schema does not know. A proxy documented for both directions follows its own attributes.
- A struct with both `tag` and `deny_unknown_fields`: serde writes the tag and rejects a request
  carrying it, so the schema neither declares the tag nor closes the object.
- Internally tagged unit and newtype variants, where serde accepts unknown keys.
- A parent that flattens such a type: `deny_unknown_fields` applies to the type, and serde accepts
  unknown keys at the parent's level. A value with the attribute that a flattened enum's branch
  wraps is documented inline and open there.

---

# Unreleased: from and into proxies document the type

A type that converts through one proxy in both directions, `#[serde(from = "P", into = "P")]` or
`#[serde(try_from = "P", into = "P")]`, is now documented as `P`, because that is what is on the
wire. It used to document its own fields. Its component stays, under its own name and with its own
documentation, and holds `P`'s schema.

**The proxy must implement `Schematic`**, or the derive no longer compiles. Derive it on the proxy,
or implement it by hand. A generic proxy such as `into = "Raw<T>"` needs a matching bound on the
type's own parameters. A proxy from another crate, such as `(u8, u8)`, cannot get an implementation
because of the orphan rule: implement `Schematic` for the type itself and drop the derive.

```rust
#[derive(Schematic, Serialize, Deserialize, Clone)] // `Schematic` is new here
struct RawColor {
    hex: String,
}

#[derive(Schematic, Serialize, Deserialize, Clone)]
#[serde(from = "RawColor", into = "RawColor")]
struct Color {
    red: u8,
    green: u8,
    blue: u8,
}
```

- Any other conversion is unchanged and needs no bound: a one-sided `from`, `try_from` or `into`, and
  a `from` or `try_from` naming another type than `into`. The type's own fields stay. One schema
  cannot describe both directions there, and the fields may be what a direction uses. The two names
  are compared as written, so `from = "Raw"` and `into = "self::Raw"` count as different types.
- `Valid` errors are unchanged: they keep reporting the fields of the converted value.
- A field of a proxy that serde writes and reads under different names documents one of them. If the
  type's own field used the other name, that direction is no longer documented.

Regenerate clients built from such a type.

---

# Unreleased: moving from GotchaApp to the builder

Every `GotchaApp` method now has a `Gotcha` builder equivalent, so an application can be described
with the builder alone. Nothing changes for existing trait applications. The table maps each method;
[the differences below](#differences-to-account-for) matter when moving an application over.

| `GotchaApp` | `Gotcha` builder |
| --- | --- |
| `type State`, `type Config` | `Gotcha<S, C>`, from `Gotcha::from_state_fn(..)`, `with_types::<S, C>()`, `from_state(value)`, or `from_context(context)` |
| `config()` override | `.load_config(\|\| async { .. })` |
| `config_error_policy()` | `.config_error_policy(policy)` |
| `logger()` | `.with_default_logger()` for the trait's default subscriber, or `.logger(\|\| { .. })`; `gotcha::default_logger()` when serving an `into_router` router yourself |
| `state(&config)` | `Gotcha::from_state_fn(\|config: Arc<ConfigWrapper<C>>\| async move { .. })` |
| `routes(router)` | `.get(..)`, `.route(..)`, `.method_route(..)`, `.fallback_service(..)`, `.nest(..)`, `.merge(..)`, or `.routes(\|router\| ..)` with the same body |
| `tasks(scheduler)` | `.try_tasks(\|scheduler\| Box::pin(async move { ..; Ok(()) }))`, or `.tasks(\|scheduler\| ..)` when registration neither awaits nor fails |
| `extractor_configs()` | `.extractor_configs(configs)`, or `.extractor_config(config)` per provider |
| `finish_router(router, context)` | `.finish_router(\|router, context\| ..)`, or `.try_finish_router(..)` for a hook that can fail |
| `build_router(context)` | `.into_router(context)` |
| `openapi_endpoints()` | `.with_openapi()` or `.openapi_endpoints(..)` |
| `openapi_info()` | `.openapi_info(info)` |
| `document_extractor_errors()` | `.document_extractor_errors(bool)` |
| `openapi_document()` | `.into_openapi()` on a freshly built application |
| `metrics_endpoint()` | `.metrics(endpoint)` |
| `shutdown_signal()` | `.shutdown_signal(future)` |
| `http_shutdown_timeout()`, `task_shutdown_timeout()` | `.http_shutdown_timeout(duration)`, `.task_shutdown_timeout(duration)` |
| `on_shutdown(&context)` | `.on_shutdown(\|context\| async move { .. })` |
| `run()`, `bind()` | `.run()`, `.bind()` |

The hooks run at the same startup stages: logger, configuration, bind, state, routes, tasks.
`gotcha/tests/builder_parity.rs` runs one application described both ways and compares them.

```rust,ignore
// Before
struct App;

impl GotchaApp for App {
    type State = AppState;
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<AppState, Config>>) -> GotchaRouter<GotchaContext<AppState, Config>> {
        router.get("/", hello_world)
    }

    async fn state(&self, config: &ConfigWrapper<Config>) -> GotchaResult<AppState> {
        AppState::connect(&config.database_url).await
    }
}

App.run().await?;

// After
fn app() -> Gotcha<AppState, Config> {
    Gotcha::from_state_fn(|config: Arc<ConfigWrapper<Config>>| async move { AppState::connect(&config.database_url).await })
        .with_default_logger()
        .get("/", hello_world)
}

app().run().await?;
```

## Differences to account for

**Logging.** The builder installs no `tracing` subscriber unless asked; trait applications got one
from the default `logger()`. Add `.with_default_logger()` for `run()` and `bind()`. Nothing installs
a subscriber on the `into_router` path, so an application that serves the assembled router itself
calls `gotcha::default_logger()?` first, where its trait version called `self.logger()?`.

**Repeated use.** A builder is consumed by `run`, `bind`, `into_router`, and `into_openapi`, while a
trait value could be used again. Write a function that returns the builder, as above, and call it
for each use: `app().into_openapi()` in an export command, `app().into_router(context)` per test.

**Borrowed data.** Builder hooks are `Send + 'static` closures. Data a trait method borrowed from
`self` is moved or cloned into the closure; share it behind an `Arc`. Two places can still borrow:
`BoundServer::serve_with_shutdown` accepts a signal future that is not `'static`, and cleanup that
needs borrowed data can run after `serve().await` returns instead of in `on_shutdown`.

**When settings are read.** The trait calls `routes`, `openapi_endpoints`, `http_shutdown_timeout`,
and its other settings methods after `config()` and `state()`, so they can read values those two
stored on `self`. Builder settings are given before startup. When a setting depends on the
configuration, load the configuration first, handle a loading error yourself (`config_error_policy`
only covers loading during startup), and build the application from it with `.config(loaded)` or
`Gotcha::from_context`. Documented routes that depend on the state or on the actual bound port are
also registered before startup, so they need both before the builder exists: bind your own
listener, build the state, register the routes from it, and serve `into_router` on that listener.
`finish_router` hooks see the context, but the routes they add are not documented.

**The `into_router` path.** Builder methods only record settings; there are no getters, so keep the
values you need in your own variables. `into_router` assembles the router and nothing else:
registrations from `tasks`/`try_tasks`, `on_shutdown` hooks, the shutdown signal, and the logger
are dropped. A trait application serving `build_router` itself could still call `tasks()` and
`on_shutdown()`; with the builder, start a `TaskScheduler::new(context.clone())` and run the
cleanup after serving in that code instead.

**Configuration types.** `from_state_fn` and `with_types` need `C: Deserialize`, as `GotchaApp::run`
does, even when `load_config` replaces automatic loading. A configuration without serde traits
starts from `Gotcha::from_context`, optionally with `load_config`. `from_state_fn` receives the
configuration as `Arc<ConfigWrapper<C>>`, the same value handlers later share.

**Synchronous assembly.** `into_router`, `finish_router`, and `try_finish_router` do not `await`,
while a `build_router` override could. Work that an override did and that can fail goes into
`try_finish_router`. Work that must `await` goes into `from_state_fn`, or before `into_router` when
you build the context yourself; `finish_router` and `try_finish_router` then mount services from
`context.state`. `from_state_fn` runs before the context exists, so it has no shutdown token: a
startup step that awaits, can fail, and needs `GotchaContext::shutdown` has no builder equivalent
on the `run`/`bind` path. It is the one capability of a `build_router` override the builder does
not cover.

---

# Unreleased: extractor provider keys

Rejection configurations and their documentation are keyed by the provider's type, so an extractor
defined outside Gotcha can be configured and documented like the built-in ones. Documents,
rejection responses, and the values the built-in extractors report do not change.

- `ExtractorKind` gains `Provider(ProviderKey)`, built by `gotcha::extract::kind::<P>()` for any
  `gotcha::extract::RejectionProvider`, and `All(KindList)`, built by `ExtractorKind::all(..)`,
  which stands for every kind another extractor reports. The enum was already
  `#[non_exhaustive]`, so `match` is unaffected, but it now has variants that carry data:
  **casting it with `as` (`ExtractorKind::Json as u8`) no longer compiles.** `ExtractorKind` is
  not in a published release yet.
- `kind::<JsonProvider>()` documents exactly like `ExtractorKind::Json`, including the
  application's `JsonConfig`, and likewise for the other built-in providers. The two are still
  different values: Gotcha's own extractors keep reporting the unit variants, so code that matches
  on `extractor_kinds()` is unaffected.
- New providers, including hand-written `extractor_kinds()`, should be named with
  `gotcha::extract::kind::<P>()`. The unit variants stay supported. A later major release will
  switch the built-in extractors to keys. See
  [Writing an extractor provider](README.md#writing-an-extractor-provider) for an extractor
  defined outside Gotcha.

---

# Unreleased: skipped and untagged variants

The `Schematic` derive now reads `#[serde(skip)]` and `#[serde(untagged)]` on enum variants.

- A `#[serde(skip)]` variant leaves the document, and its type no longer needs `Schematic`. If only
  unit variants remain, an externally tagged enum becomes a plain string enum. A variant skipped in
  one direction stays documented.
- An `#[serde(untagged)]` variant of a tagged enum is documented without the tag. The enum's
  branches change from `oneOf` to `anyOf` and its `discriminator` goes away, because the untagged
  branch has no tag and can accept what another branch accepts.
- An externally tagged all-unit enum with an untagged variant changes from a string enum to
  `anyOf`, because that variant is written as `null`.
- An `#[serde(untagged)]` variant of an internally tagged enum documents a wrapped struct through
  its fields, as that enum's other newtype variants do. `Option<S>` and a struct flattening an enum
  stay a `$ref`, so `S` becomes a component. A type with the same name elsewhere in the document
  then gets a qualified component name, also on routes that do not use this enum.

Generated types change for these enums, also for code that only uses the tagged variants, so
regenerate clients built from them.

---

# Unreleased: names serde writes and reads differently

The `Schematic` derive now reads the list forms of `rename`, `rename_all` and `rename_all_fields`.
A type has one schema, so it cannot describe both directions when the names differ. A name stays
when it is already right in one direction, and otherwise becomes the name serde writes:

| Attribute | Schema name before | Schema name now | serde reads |
| --- | --- | --- | --- |
| `rename(serialize = "same", deserialize = "same")` | field name | `same` | `same` |
| `rename(serialize = "out", deserialize = "in")` | field name | `out` | `in` |
| `rename(serialize = "out")` | field name | field name | field name |
| `rename(deserialize = "in")` | field name | field name | `in` |

The conventions behave the same way. `rename_all(serialize = "camelCase", deserialize = "snake_case")`
keeps a snake_case field's name, because that is what serde reads.

Query and path parameters now use the name serde reads. They used the schema's name, so
`rename(deserialize = "in")` documented a parameter serde does not read. `Valid` errors are
unchanged, and hand-written `Schematic` implementations and `WireField` tables keep working.

Regenerate clients built from a type with such a form: the old document named a key serde neither
writes nor reads.

---

# Unreleased: the macro crate needs Rust 1.88

`gotcha_macro` now depends on darling 0.23, which declares `rust-version = "1.88"`, so a
dependency tree with only `gotcha_core` or `gotcha_macro` needs Rust 1.88 (previously 1.85).
`gotcha` already resolved darling 0.23 through `validator`, so its requirement is unchanged.

---

# Unreleased: untagged enums use anyOf

`#[serde(untagged)]` enums are now documented with `anyOf` instead of `oneOf`. Serde picks the
first variant that accepts a value, and more than one branch may accept the same value
(`u32 | f64` with `1`, or struct variants whose fields are subsets), so `oneOf` rejected values
serde serializes successfully.

Every untagged enum's generated schema and OpenAPI document changes (`oneOf` → `anyOf`).
Generated clients see "matches at least one branch" instead of "matches exactly one" for those
fields, so regenerate them.

---

# Unreleased: variant-level schematic attributes

Enum variants accept `#[schematic(title = "...", description = "...")]`. The annotations are
added to the variant's schema branch, and a doc comment on the variant is the description
fallback (an explicit `description` wins). Only these two keys are accepted; other keys are now
compile errors, where they previously compiled and were ignored.

Generated documents change for enums with doc-commented or annotated variants: those variants
gain a `description`/`title` they did not have. Regenerate clients built from them. Doc comments
on tuple/newtype *fields* are still not descriptions (only explicit attributes document those);
named struct-variant fields keep their doc comments, as before.

---

---

# Unreleased: derive documents serde's wire shapes

The `Schematic` derive now documents enum variants and struct-variant fields the way serde writes
them. Generated documents change for these shapes; regenerate clients built from them.

- Unit variants follow the tag form: an all-unit internally or adjacently tagged enum is an
  object (`{"kind": "A"}`), an untagged unit variant is `null`, and a unit variant in a mixed
  externally tagged enum is the bare string. Zero-field struct (`A {}`) and tuple (`A()`) variants
  keep their object/array shapes; a few unit combinations remain documented as known mismatches.
- Untagged enum struct-variant fields keep their Rust names; the container `rename_all` names
  variants, which untagged serialization never writes.
- Struct-variant fields honour `skip`/`skip_serializing`/`skip_deserializing`, `flatten`, the
  variant's `rename_all` and the container's `rename_all_fields`, in every tag form.
- A multi-field tuple variant is documented as an array (`prefixItems` with pinned
  `minItems`/`maxItems`) instead of an empty object, and tuple-field `#[schematic(..)]`
  annotations are applied.
- `#[serde(transparent)]` structs document their single non-skipped inner value. The wrapper no
  longer gets a component of its own and its own doc comment no longer appears.
- Flattened maps add `additionalProperties` to the parent (or `unevaluatedProperties` next to an
  `allOf`); an internally tagged newtype variant wrapping a map merges its entries next to the
  tag.
- A tuple/newtype field's `#[schematic(..)]` applies to the value it documents. On an
  internally tagged newtype variant only `title`/`description` can go on the object branch;
  `example`/`default`/`format` need a value schema. Doc comments on tuple/newtype variant fields
  are not turned into descriptions; named struct-variant fields keep theirs, as before.

---

# Unreleased: invalid task registrations fail startup

**An application that registers an invalid cron expression or a zero interval no longer starts.**
`TaskScheduler::cron` and `interval` previously logged the problem and dropped the task, so
`bind()` and `run()` succeeded with a silently missing schedule. Registration problems are now
returned from preparation as a `GotchaError`: the message names the task and, for cron, the
expression, and the parse error stays available through the error's source chain. The listener is
released and no pending task starts.

Standalone `TaskScheduler::start()` callers keep the previous behaviour: recorded registrations are
logged and skipped, and the valid tasks still start. Fix the expression or interval before
upgrading.

---

# Unreleased: non-exhaustive extractor enums

**`gotcha::ExtractorKind` (with the `openapi` feature), `gotcha_core::ExtractorKind` (with
`axum`), `gotcha::validation::ValidRejection`, and `gotcha::ParamRejection` are now
`#[non_exhaustive]`,** so a later release can add an extractor provider, a validation-failure
variant, or a header/cookie rejection variant without a breaking change.

- A `match` on any of these types outside the crate that defines it needs a wildcard arm
  (`_ => ..`); a `matches!(..)` with a single variant keeps compiling.
- Constructing the existing variants is unchanged: `ExtractorKind::Json`, the `ValidRejection`
  variants, and `ParamRejection::Missing`/`Invalid`/`NotText` remain usable.

---

# Unreleased: shared configuration

`GotchaContext::config` is now an `Arc<ConfigWrapper<C>>` rather than a `ConfigWrapper<C>`. axum
clones the state passed through `with_state` for every request, so the previous representation
deep-cloned the whole application configuration on each request; a request now clones the `Arc`
instead. How much this saves grows with the configuration: string fields dominate.

- `GotchaContext::new(state, config)` keeps its signature and wraps the configuration in an `Arc`.
- Reading is unchanged: `context.config.server.port` and `context.config.field` still work through
  `Arc`'s `Deref`, `&*context.config` yields `&ConfigWrapper<C>`, and extractors such as
  `State<ConfigWrapper<C>>`, `State<ServerConfig>`, and the `#[config]` types are unaffected. Deref
  coercion also keeps `let config: &ConfigWrapper<C> = &context.config;` compiling.
- Writing through the field no longer compiles (`context.config.app.field = ..` is E0594: the
  `Arc` only hands out shared access). Use `Arc::make_mut(&mut context.config).app.field = ..`
  while the configuration implements `Clone`, or `Arc::get_mut(&mut context.config)` while the
  `Arc` is unique (no `Clone` needed). For a shared, non-`Clone` configuration, rebuild a
  `ConfigWrapper` from its parts and assign `context.config = Arc::new(config)`.
- Serializing the field needs a dereference: `serde_json::to_string(&context.config)` and
  `Json(context.config.clone())` stop compiling because serde's `rc` feature is off by default. Use
  `serde_json::to_string(&*context.config)` and `Json((*context.config).clone())`.
- Cloning a context no longer requires `Config: Clone`: the shared configuration is only refcounted.
- `GotchaContext` is `Send`/`Sync` only when the stored `ConfigWrapper<C>` is `Send + Sync`, because
  `Arc<T>` requires both; a hand-built context whose configuration was only `Send` is no longer
  `Send`. Application configurations are normally `Send + Sync`.
- `Gotcha::from_context` reuses the supplied configuration without cloning when nothing else
  shares it.

```rust,ignore
// Before
fn app_config(context: &GotchaContext<AppState, AppConfig>) -> ConfigWrapper<AppConfig> {
    context.config.clone()
}

// After
fn app_config(context: &GotchaContext<AppState, AppConfig>) -> ConfigWrapper<AppConfig> {
    (*context.config).clone()
}
```

---

# Unreleased: removed unused public items

Several public items that nothing in the framework, examples, or documentation used are gone.
Each had a direct replacement:

| Removed | Replacement |
| --- | --- |
| `Gotcha::quick_start()` | `Gotcha::new()`, plus logging setup if you need it |
| `quick_server!` | `Gotcha::new()` followed by the same route method calls |
| `handler!` | write the closure directly, for example an `async move` block |
| `gotcha::Lazy` | `once_cell::sync::Lazy` (or `std::sync::LazyLock`) with a direct dependency |
| `GotchaConfig` | name the bounds directly: `Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default` |
| `gotcha::prelude::Result` | write `Result<T, Box<dyn std::error::Error + Send + Sync>>`; standard `Result` is no longer shadowed |
| `gotcha::openapi::replace_path_variable` | nothing: axum 0.8 routes already use OpenAPI's `{name}` syntax |
| `gotcha::openapi::path_variable_names` | `axum::extract::MatchedPath`, or iterate the `{name}` segments yourself |

`Gotcha::quick_start` panicked when called twice because it installed the tracing subscriber
globally; logging is left to the application. `quick_server!` was documented as starting a server
but only returned a builder. `prelude::Result` shadowed the standard library's `Result` on glob
import, so `Result<T>` in a module using the prelude failed to mean `std::result::Result`.

gotcha's direct `regex` dependency is gone with `path_variable_names` (`gotcha_core` still uses
its own `regex` dependency). `EmptyState` stays: it appears in the builder's default type
parameters and public signatures, and replacing it with `()` would churn call sites without
removing a concept.

---

# Unreleased: duplicate operation ids are suffixed

**Two operations can no longer emit the same `operationId`.** The id defaults to the handler's
function name, so `users::list` and `books::list`, or one handler mounted at two paths, used to
collide. The first operation now keeps the id and each later duplicate gets `_2`, `_3`, … in the
order operations appear in the document: by path, and by method as `PathItem` writes it (GET,
PUT, POST, DELETE, OPTIONS, HEAD, PATCH, TRACE). Explicit `#[api(id = "…")]` ids follow the same
rule, and an existing id is never renamed by a generated suffix.

Because a suffix depends on the order of the operations, adding a route can rename an existing
operation in the generated document (and therefore in clients generated from it). Give operations
an explicit `#[api(id = "…")]` when their ids must stay stable. Documents whose ids were already
unique are unchanged.

---

# Unreleased: data-type Schematic impls are features

**`gotcha_core`'s `Schematic` implementations for `bigdecimal`, `chrono`, `rust_decimal`, and
`uuid` are now default features of `gotcha_core`.** A default dependency behaves exactly as
before, and `gotcha` forwards the four features from its own default set, so applications that
just depend on `gotcha` keep every implementation. This only affects manifests that already pass
`default-features = false` today: in 0.4.x the only default feature was `http1`, so some
applications do; they now also lose the data-type implementations unless they list them.
Nothing changes for a manifest that leaves default features enabled.

- `gotcha_core = { ..., default-features = false }` no longer implements `Schematic` for
  `uuid::Uuid`, `bigdecimal::BigDecimal`, `rust_decimal::Decimal`, or the chrono types. List the
  implementations you use, for example
  `features = ["uuid", "chrono"]`, or leave the default features enabled.
- `gotcha = { ..., default-features = false }` no longer enables those forwarded implementations
  either; add `bigdecimal`, `chrono`, `rust_decimal`, or `uuid` to its feature list. `chrono`
  remains a direct dependency for task scheduling even without the feature; only its `Schematic`
  implementations are affected.

Applications that never document these types can now leave them out with
`default-features = false`, which drops the corresponding crates (and their dependencies).

---

# Unreleased: validation `length` keywords follow the schema type

`#[validate(length(min = .., max = ..))]` now documents the keywords of the schema the field
serializes to, chosen at runtime:

- strings → `minLength`/`maxLength`
- arrays, sets and deques → `minItems`/`maxItems`
- maps → `minProperties`/`maxProperties`

String and `Vec<_>` fields keep their existing keywords. Documents change for fields whose Rust
type is a set, a deque or a map (previously `minLength`/`maxLength`, now items/properties), and
for types the old source-text check could not see through: `Box<Vec<_>>`, a `Vec` type alias, a
newtype wrapping a `Vec`, and `[T; N]` (previously `minLength`/`maxLength`; now
`minItems`/`maxItems`, merged with the array's exact length so the tighter bound wins).

Generated clients that pinned the old keywords on such fields must be regenerated.

---

# Unreleased: extractor rejection responses in OpenAPI

**Operations now document the rejections of Gotcha's extractors by default.** An operation whose
arguments use `Json`, `Form`, `Query`, `Path`, `Multipart`, `TypedHeader`, `Header`, `Cookie`, or
`Valid` gains a response for every status that provider can reject with, so an existing document
changes even when the handler code does not. The body is the type configured for that provider:
`ErrorBody` by default, or the type passed to `XxxConfig::error_handler`.

Default statuses, checked against Axum's rejection types:

| Provider | Statuses |
| --- | --- |
| `Json` | `400`, `413`, `415`, `422` |
| `Form` | `400`, `413`, `415`, `422` |
| `Query` | `400` |
| `Path` | `400` |
| `Multipart` | `400` |
| `TypedHeader`, `Header`, `Cookie` | `400` |
| `Valid` | `422`, plus whatever its inner extractor declares |

`413` is Axum's body-limit rejection, and `Form` also uses `400` when a `GET`/`HEAD` form fails
to deserialize. `Path` documents only `400`: Axum's other variant, `MissingPathParams` (`500`),
requires the router's `UrlParams` extension to be absent, which cannot happen for a matched
`#[api]` route. An application that can still observe it can declare `500` with
`.statuses([..])` on `PathConfig` or with `errors(...)`. `Valid<Json<T>>` documents the JSON
statuses and its own `422`; `Valid<axum::extract::Json<T>>` documents only `422`, because Axum's
original `Json` keeps its plain-text rejection.

- Several providers can share a status. When they use the same configured body, one response is
  emitted; different bodies become `oneOf` references under `application/json`. Descriptions list
  the causes in handler-argument order, such as
  `Request rejected: invalid query string / invalid JSON body`.
- A status declared in `#[api(errors(...))]` or `#[api(responses(...))]` wins over the automatic
  one for that status. Inferred success responses also keep their status.
- `skip_response` still replaces return-type inference only. Extractor rejection responses are
  added, and explicit `responses(...)` entries still win. Its nonempty `responses(...)` list is
  no longer the whole contract when the handler uses Gotcha extractors.
- Axum's own extractors (`gotcha::axum::extract::*`, `gotcha::axum_extra::TypedHeader`) and
  custom `ParameterProvider` implementations report no providers and are unchanged. `Option<T>`
  forwards its inner extractor's rejections: an optional JSON body is still rejected when it is
  present but malformed.
- A body replaced with `#[api(request_body(...))]` keeps the rejections of the extractor that
  actually runs.
- An argument marked `#[api(skip)]` contributes neither parameters nor rejection responses, and
  its `ParameterProvider` is not consulted at all. Declare any rejection it can send with
  `errors(...)` or `responses(...)`.
- `ErrorBody` now reserves the component name `ErrorBody` even when an application defines a type
  with the same name; the application's type receives a module-prefixed name instead.
- Static `Operable` descriptors gain an `extractors` field, one
  `<T as ParameterProvider>::extractor_kinds` constructor per documented argument (an empty slice
  is valid). `ParameterProvider::extractor_kinds` has a default returning an empty slice, so
  custom implementations need no change. The doc-hidden
  `generate_openapi_with_schemas{,_and_info}` helpers are replaced by
  `generate_openapi_with_configs_and_info`.

A custom `error_handler` can declare the statuses it actually returns. `.statuses(..)` replaces
the documented set for that provider; without it, the provider's defaults above are documented:

```rust,no_run
use gotcha::axum::http::StatusCode;
use gotcha::{ErrorBody, Gotcha, JsonConfig};

let app = Gotcha::new().extractor_config(
    JsonConfig::default()
        .error_handler(|rejection| (StatusCode::BAD_REQUEST, ErrorBody::new(rejection.body_text())))
        .statuses([StatusCode::BAD_REQUEST, StatusCode::CONFLICT]),
);
```

Opt out for the whole application with the builder, the trait hook, or the router. The setting
applies to served and exported documents:

```rust,no_run
let app = gotcha::Gotcha::new().document_extractor_errors(false);
// Or: return false from `GotchaApp::document_extractor_errors`.
// Or: `GotchaRouter::default().document_extractor_errors(false)`.
```

Regenerate or republish checked-in documents and generated clients.

---

# Unreleased: neutral OpenAPI info defaults

**Generated documents no longer identify themselves as gotcha.** The default `info` block is now
neutral: `title: "API"`, `version: "0.1.0"`, and no description (every other optional field is
absent). Previously every application's document said `title: "Gotcha"`,
`description: "Gotcha is a framework for building microservices"`, and `version: "1.0.0"`.

Set the application's own identity with `.openapi_info(..)` on the builder, or by overriding
`GotchaApp::openapi_info`. The builder setting feeds the served document and every offline export
(`Gotcha::into_openapi`, `GotchaRouter::into_openapi`). The trait hook feeds the served document and
`GotchaApp::openapi_document`; exporting straight from `routes(..)` requires passing it on:
`app.routes(GotchaRouter::default()).openapi_info(app.openapi_info()).into_openapi()`.
`gotcha::cargo_info!()` fills the block from the calling crate's Cargo metadata:

```rust,no_run
let app = gotcha::Gotcha::new().openapi_info(gotcha::cargo_info!());
```

To keep a specific title and version, set the block explicitly or edit it in a transform:

```rust,no_run
let app = gotcha::Gotcha::new().openapi_info(gotcha::oas::Info {
    title: "My Service".into(),
    description: Some("Service API".into()),
    terms_of_service: None,
    contact: None,
    license: None,
    version: "1.0.0".into(),
});

let app = gotcha::Gotcha::new().openapi(|mut spec| {
    spec.info.title = "My Service".into();
    spec.info.version = "1.0.0".into();
    spec
});
```

An existing `.openapi(|spec| ..)` transform still runs afterwards and can override the block, which
is the migration path for applications that relied on the old branding. Regenerate or republish
checked-in documents if clients or tooling read `info`.

---

# Unreleased: serde-driven field presence

`#[derive(Schematic)]` now derives a named field's presence and nullability from the serde
attributes already on it, so the document follows the contract for most patterns. Plain
`Option<T>` and fields without these attributes keep their current schemas. One asymmetric
combination keeps its previous schema; see the last bullet.

- `#[serde(deserialize_with = "...")]` or `#[serde(with = "...")]` without a `default` makes the
  field required, including `Option<T>`. Serde rejects a missing key before the option can
  produce `None`, so the field is listed in `required`; an `Option<T>` field is still nullable.
  Previously every `Option<T>` field was optional, so a struct that used `deserialize_with` to
  require an option now documents it as required.
- `#[serde(skip_serializing_if = "Option::is_none")]` on an `Option<T>` field remains optional
  and now drops the `null` branch: `None` is omitted and never serialized as `null`. The
  recognized predicates are `Option::is_none`, `std::option::Option::is_none` and
  `core::option::Option::is_none`. Any other predicate keeps the current behavior (optional,
  with the null branch).
- `#[serde(default)]` on a struct container makes every direct field optional, matching serde.
  Fields merged by `#[serde(flatten)]` keep the inner type's requiredness. Field-level `default`
  behaves as before.
- `deserialize_with` or `with` combined with a recognized `Option::is_none` skip, without a
  `default`, stays optional and nullable as before. Input requires the key while output may
  omit it, which a single schema cannot express; use `default` with the tri-state pattern to
  make the contract consistent.
- `Option<Option<T>>` renders one `anyOf` with a single null branch instead of a nested `anyOf`
  of `anyOf`. With `default` + `deserialize_with` + `skip_serializing_if = "Option::is_none"` it
  documents the tri-state contract: optional, nullable, and never more than one null branch.

Regenerate clients where a field became required or lost its null branch. No source changes are
needed unless a type relied on the previous document shape.

---

# Unreleased: validation error bodies

A `Valid` validation failure still returns `422 Unprocessable Entity`, but its body is now the
framework-wide `ErrorBody` instead of validator's raw `ValidationErrors` structure:

```json
{
  "error": "validation failed",
  "fields": {
    "address.city": ["length must be at least 1 and at most 64"],
    "items[0].name": ["length must be at least 1"]
  }
}
```

- Nested struct fields join with `.` (`address.city`), list elements use `[n]`
  (`items[0].name`), and struct-level (schema) validations keep validator's `__all__` key
  (`address.__all__` for a nested struct).
- Every message is non-empty: the declared `#[validate(.., message = "..")]` text or the same
  readable default the previous body filled in.
- `ErrorBody` gains `fields: Option<BTreeMap<String, Vec<String>>>`, serialized only when
  `Some`. Every other provider's body is unchanged. Code that constructs `ErrorBody` with a
  struct literal must add `fields: None`; `ErrorBody::new(error)` and
  `ErrorBody::with_fields(error, fields)` avoid depending on the struct's fields.
- A value validated directly as a collection keeps no field name: `Valid<Json<Vec<T>>>` reports
  `[0].name`, and a collection of collections `[0][1].name`, instead of validator's internal
  `_tmp_validator` key.
- Values of a nested map (`HashMap`/`BTreeMap` with `#[validate(nested)]`) appear as
  `entries[<index>].name`. Validator indexes them by map iteration order and drops the key, so
  the original key cannot be recovered and the index may differ between requests.
- Field segments are serde wire names, not Rust identifiers: `first_name` sent as `firstName` is
  reported as `firstName`. See
  [Schematic in every build and wire-name validation errors](#unreleased-schematic-in-every-build-and-wire-name-validation-errors).
- The unreleased `ValidationErrorsBody` type is removed. A custom
  `ValidConfig::error_handler` still receives `&validator::ValidationErrors`; use
  `gotcha::validation::flatten_errors(&errors)` to build the same field map (or any other
  response shape).
- `ValidationErrors` is no longer registered as an OpenAPI component. The `Valid` default now
  sends `ErrorBody`, so a document that references `ErrorBody` shows `fields` as an optional
  object of string arrays (no null branch). Documents that never reference `ErrorBody` are
  unchanged.

Clients that parsed the raw `ValidationErrors` structure must move to the `fields` map.

---

# Unreleased: Schematic in every build and wire-name validation errors

**`Schematic` no longer requires the `openapi` feature, and `Valid` reports failures under the
names serde reads.**

- `gotcha_core` is now a required dependency of `gotcha`. The `openapi` feature only enables
  document generation and its axum-facing glue: `oas`, `#[api]`, `ParameterProvider`,
  `Responsible`, `gotcha_core/axum`, and the documentation endpoints. A build with no features
  gains `gotcha_core`, `oas`, `serde_with`, and `serde_with_macros`; chrono, uuid, bigdecimal,
  and rust_decimal were already dependencies of `gotcha`.
- `Schematic`, `#[derive(Schematic)]`, `EnhancedSchema`, and the new `WireField` are re-exported
  at the crate root and in the prelude in every build. `gotcha::gotcha_core` is always available
  too, so the derive's generated paths resolve without `openapi`.
- `Valid<Json<T>>` and `Valid<axum::extract::Json<T>>` require `T: Schematic` (container types
  such as `Vec<T>` implement it by delegating). Add `#[derive(Schematic)]` to the validated type
  if it only derived `Validate` before. A hand-written `Schematic` implementation keeps
  compiling; add `wire_fields` when its fields are renamed on the wire. A hand-written wrapper
  must forward to its inner type (`T::wire_fields()`): the default returns no mapping, so a
  generic wrapper such as `Paged<T>` that forgets to delegate hides every renamed field below it.
- The new blanket `impl<T: Schematic> Schematic for Box<T>` conflicts with a downstream
  `impl Schematic for Box<LocalType>` when `LocalType` also implements `Schematic`. `Box` is
  `#[fundamental]`, so the orphan rules already allowed that manual implementation and the
  compiler now reports E0119. Delete it: `Box<T>` delegates to `T` automatically. The other new
  container implementations (`Arc`, `Rc`, `Cow`, sets, `VecDeque`, arrays) are not fundamental,
  so a downstream implementation for them was never allowed and cannot conflict.
- Validation error paths now use serde's *deserialize* names at every segment:
  `#[serde(rename = "..")]`, `#[serde(rename_all = "..")]`, and the
  `rename(deserialize = "..")` / `rename_all(deserialize = "..")` forms. A `first_name` field
  sent as `firstName` is reported as `firstName`. `__all__` and the fields of a type without a
  `Schematic` table keep their names, and nested maps keep their index-only paths. A
  `#[serde(skip_serializing)]` field is renamed, but the fields below it keep their Rust names,
  because its type is not required to implement `Schematic`.
- A `#[serde(flatten)]` struct field contributes no path segment: its fields are reported at the
  parent level, matching the payload shape.
- The rewrite happens before rendering, so the default `ErrorBody`, a custom
  `ValidConfig::error_handler`, and `Rejected<ValidRejection>` all see wire names. Code that calls
  `validate()` itself can apply the same mapping with
  `gotcha::validation::remap_to_wire_names::<T>(&mut errors)`.
- `#[schematic(as = "Type")]` describes a field as another type for the schema, requiredness,
  nullability, and nested wire mapping; the declared type no longer needs `Schematic`. Serde
  presence rules still apply on top; an `Option<MyUrl>` field with `as = "Option<String>"`
  keeps its nullable schema.
- New `Schematic` implementations: `Box<T>`, `Arc<T>`, `Rc<T>`, `Cow<'_, T>`, `str`, `char`,
  `i128`/`u128`, the `NonZero*` integers (`minimum: 1` for unsigned), `IpAddr`/`Ipv4Addr`/
  `Ipv6Addr`, `DateTime<FixedOffset>`, `DateTime<Local>`, `NaiveTime`, `BTreeSet<T>`,
  `VecDeque<T>`, and `[T; N]` (with `minItems`/`maxItems` set to `N`).

Documents for applications that do not use `#[schematic(as = "..")]` are unchanged. Clients that
read `fields` keys may see renamed keys; regenerate clients that pinned the Rust names.

---

# Unreleased: JSON error bodies and extractors

**Every extractor provider now has its own rejection configuration.** The defaults keep Axum's
own status for that rejection (`400`, `415`, `422`, ...) and send `{"error": "<message>"}` with
`Content-Type: application/json`.

`gotcha::Json`, `gotcha::Path`, `gotcha::Query`, `gotcha::Form`, `gotcha::Multipart`, and
`gotcha::TypedHeader` — at the crate root, and the first five in the prelude — are now Gotcha's
wrappers around Axum's extractors rather than re-exports. Pattern destructuring, `Deref`,
`Json`/`Form` responses, and `Option<Json<T>>`, `Option<Path<T>>`, and `Option<Multipart>`
extraction keep working. `Header` and `Cookie` were already Gotcha extractors; their rejections
now use the same JSON body with `400`.

Default behavior by provider:

- `Json`, `Form`, `Query`, `Path`, `Multipart`, `TypedHeader`: Axum's status
  (`rejection.status()`) and `{"error": rejection.body_text()}`. For `TypedHeader` the status is
  `400`.
- `Header`, `Cookie`: `400` with the same JSON body.
- `Valid`: validation failures keep today's `422 Unprocessable Entity`, now with the shared
  `ErrorBody` plus a flattened `fields` map (see
  [validation error bodies](#unreleased-validation-error-bodies)). A malformed body inside
  `Valid<Json<T>>` is a `Json` rejection and uses the `Json` handler.

Override one provider on the builder with
`.extractor_config(JsonConfig::default().error_handler(|rejection| (status, body)))`, or in the
trait API by overriding `GotchaApp::extractor_configs` and calling
`ExtractorConfigs::default().set(...)`. The handler receives a reference to that provider's
concrete rejection type and returns `(StatusCode, E)`. `E` must be `Serialize`, and with
`openapi` also `Schematic`; every configured body is registered as an OpenAPI component when it
is referenced, and since the automatic rejection responses below document the default
`ErrorBody`, documents that use a provider gain that component too.

Configurations reach extractors through request extensions installed as the outermost layer,
after routes, documentation, `finish_router`, and `app_layer` middleware. Extractors used without
that assembly (a plain Axum router, or a direct `from_request` call) use the defaults. An
application that overrides `GotchaApp::build_router` entirely must call
`ExtractorConfigs::layer(router)` last.

**Extractor rejections are now `Rejected<R>`.** `IntoResponse` returns the configured response,
while `rejection()` and `into_inner()` expose the original rejection. Axum's per-handler pattern
still works, but the second type changes:

```rust
use gotcha::{Json, Rejected};
use gotcha::axum::extract::rejection::JsonRejection;

async fn handler(body: Result<Json<MyBody>, Rejected<JsonRejection>>) -> String {
    match body {
        Ok(Json(value)) => value.to_string(),
        Err(rejected) => rejected.rejection().body_text(),
    }
}
```

`Header`/`Cookie` use `Rejected<ParamRejection>`. `Valid` uses `Rejected<ValidRejection>`;
`ValidRejection::Json` carries the original JSON rejection and `ValidRejection::Invalid` the
validation errors. `Valid<axum::extract::Json<T>>` remains supported and keeps Axum's original
JSON rejection response. To keep Axum's exact rejection types instead, use
`gotcha::axum::extract::*` directly. Extractor rejection types keep their `From<_> for HttpError`
conversions.

`Json::from_bytes` and `Query::try_from_uri` return `HttpError`: they have no request and
therefore no configured handler; the error keeps the Axum status and body text.

Use Axum's originals to opt out of the wrappers and their configured handlers:

```rust
use gotcha::axum::extract::{Form, Json, Multipart, Path, Query};
use gotcha::axum_extra::TypedHeader;
```

**New `HttpError` and `ErrorBody`.** `HttpError` holds a status and a message and renders the
same JSON body; constructors are `new`, `bad_request`, `not_found`, `conflict`, and `internal`.
It implements `IntoResponse`, `Schematic`, and `Responsible`, so it can be returned from handlers
and declared in `#[api(errors(response(status = 404, body = "HttpError")))]` or
`#[api(responses(...))]`. The registered component is `ErrorBody { error: String, fields:
Option<BTreeMap<String, Vec<String>>> }`; either type name can be used in a declaration.
`fields` is optional and omitted when `None`; only `Valid` sets it.

**`GotchaError` no longer leaks its message.** It still renders `500 Internal Server Error`, but
the body is now `{"error": "internal server error"}`. The full error (including configuration
paths, addresses, and I/O failures) is sent to `tracing::error!` only.

**`json_response!` expands to `gotcha::Json`** and to gotcha's re-exported `serde_json` instead
of naming `axum::Json` at the call site, so callers need neither dependency directly. It also
forwards object and array literal tokens correctly, so `json_response!({"ok": true})` compiles.

---

# Unreleased: OpenAPI 3.2

Generated documents now use **OpenAPI 3.2.0** and `oas 0.3`. This changes both the exported
schema format and the public `oas` types used by custom schema/response implementations.
Consumers need tooling that understands OpenAPI 3.2 and JSON Schema 2020-12; changing the
`openapi` string back to 3.0 does not convert the schemas.

## Generated schemas

- `Option<T>` becomes `anyOf: [T, {"type": "null"}]`, including references and enums.
  The shared `gotcha_core::nullable_schema` helper preserves non-null constraints and never
  changes the referenced component. `Schematic::nullable()` remains available and the default
  generator uses the same helper. Existing field-presence rules are unchanged.
- Exclusive validation bounds become numeric `exclusiveMinimum` / `exclusiveMaximum` values,
  replacing the 3.0 boolean flags. Inclusive bounds remain `minimum` / `maximum`.
- `#[schematic(example = ...)]` keeps its source syntax and JSON value type, but emits a
  one-element schema `examples` array. Media-type examples keep their OpenAPI representation.
- `Json<()>` has a `{"type":"null"}` schema. A bare `()` response still has no body.
- Raw binary responses use `application/octet-stream` with an unconstrained `{}` schema,
  without asserting that the bytes are a JSON string or using the old `binary` format.

## Documentation viewers

The default documentation endpoints are now `/openapi.json` and `/scalar`. The bundled Redoc
rejects 3.2 documents, so `redoc_path` defaults to `None`; the explicit configuration field is
retained. Use Scalar for generated documents.

Browser verification confirmed that Scalar loads 3.2 documents and renders ordinary responses,
nullable references, enums, and examples. It currently displays an `itemSchema`-only SSE response
as "No Body"; stream-item rendering is not supported by the tested UI. Consumers can inspect
`itemSchema` in the exported JSON. Scalar continues to load from its existing CDN URL.

## Custom schemas and responses

`Schematic` and `EnhancedSchema` remain in `gotcha_core`; standalone libraries do not need
Axum or the gotcha facade. Existing derive dependency aliases and crate overrides still work.

- `Schema._type` is now `Option<SchemaType>`: use `Some("string".into())` for a single type,
  or `SchemaType::Multiple` for a type union. Prefer `..Schema::default()` in struct literals.
- Schema-bearing fields such as `MediaType.schema`, `Parameter.schema`, and component entries
  now use `SchemaValue`, which represents an object or a boolean. Convert a `Schema` with
  `.into()`; inspect an object with `.as_object()` / `.as_object_mut()`.
- Set schema references with `Schema::reference(...)` or `Schema._ref`, rather than duplicating
  `$ref` in `extras`. Siblings such as `description` and constraints are preserved.
- `MediaType` has a new `item_schema` field; initialize it to `None` in existing literals.
  Reference collection and response unions support it, as well as `Schema.content_schema`.
  `contentMediaType` describes encoded data inside a string; it is separate from the HTTP media type.
- Custom implementations that construct schemas directly must migrate legacy `nullable`,
  exclusive bounds, examples, and binary assumptions themselves. The oas version constructor
  does not translate schemas. Use `nullable_schema` when an object schema must also admit null.

This release migrates Gotcha's generated subset, not every OpenAPI 3.2 object or keyword.
See the [oas migration guide](https://github.com/Kilerd/oas/blob/main/MIGRATION.md) for remaining
model differences. SSE responses can now be declared with `skip_response` and `item`;
WebSocket message contracts remain separate AsyncAPI work.

---

# Unreleased: HTTP/1 is always enabled

The `http1` feature has been removed. It existed to support Cloudflare Worker integration,
which is no longer supported. HTTP/1 and Tokio support are now required Axum dependency
features, so server startup remains available even with `default-features = false`.

Remove `"http1"` from any explicit gotcha feature lists. Default application behavior is
unchanged; `openapi`, `prometheus`, `cors`, `static_files`, and `task` remain optional.
Libraries that only implement `Schematic` can continue to depend on `gotcha_core` directly.

---

# Unreleased: derive dependency paths

**`Schematic` stays in `gotcha_core`.** Libraries can continue to depend only on core and either
implement the trait manually or use `#[derive(gotcha_core::Schematic)]`. Core's web integration
remains optional. Applications depending on gotcha with `openapi` enabled can now use
`#[derive(gotcha::Schematic)]` without importing the trait or adding a direct core dependency.
Both entry points implement the same trait, so library types work in application schemas.

Remove a direct `gotcha_core` dependency if it was added only to make the derive compile.
If you also use support APIs such as the registry, they are available through
`gotcha::gotcha_core`. The derive prefers a direct core dependency when present, otherwise
gotcha's re-export, and recognizes Cargo dependency renames for both. A custom facade can set
`#[schematic(crate = "path::to::gotcha_core")]` explicitly. This does not change schema names
or generated document content. Cargo dependency renames are recognized by the derive and by the
`#[api]`, `#[state]` and `#[config]` attribute macros, so a dependency renamed to `web` can use
`#[derive(web::Schematic)]`, `#[web::api]`, `#[web::state]` and `#[web::config]`. When the
calling manifest cannot be inspected (a build system without a `Cargo.toml`, or a package that
re-exports gotcha under the `gotcha` name), the attribute macros fall back to `::gotcha`.

**Validation can use gotcha's validator version directly.** To avoid a separate `validator`
dependency, select the re-export using the upstream derive's existing crate override:

```rust
#[derive(gotcha::Validate)]
#[validate(crate = "gotcha::validator")]
struct Request {
    #[validate(length(min = 1))]
    name: String,
}
```

The override is required for this form; re-exporting the derive does not change its upstream
default. If gotcha is renamed to `web`, use `#[derive(web::Validate)]` and
`#[validate(crate = "web::validator")]`. Existing direct validator dependencies still work
when their trait version matches gotcha's.

---

# Unreleased: explicit OpenAPI endpoints

**The `openapi` feature no longer exposes HTTP documentation by itself.** It enables schema and
operation metadata. Applications choose whether and where to serve the resulting document.

For the builder, add `.with_openapi()` to enable `/openapi.json` and `/scalar`.
This formerly empty method now enables the default endpoint configuration.
Use `.openapi_endpoints(Some(OpenApiEndpoints { ... }))` for custom paths, or
`.openapi_endpoints(None)` to disable all endpoints. The last configuration call wins;
`with_openapi()` resets custom paths to their defaults. `redoc_path` and `scalar_path` can each be
`None` to serve just the document or a single UI.

For the trait API, explicitly configure the application:

```rust,no_run
use gotcha::{ConfigWrapper, EmptyConfig, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, OpenApiEndpoints};

struct App;
impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    fn openapi_endpoints(&self) -> Option<OpenApiEndpoints> {
        Some(OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        router.get("/health", || async { "ok" })
    }

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> { Ok(()) }
}
```

Endpoint configuration belongs to the top-level application. Nesting or merging business routers
does not enable, relocate, or override documentation endpoints. Both UI pages use the configured
`json_path`. Paths must be distinct literal absolute URI paths, with no query, fragment, or dot
segments; percent-encode other characters. Invalid endpoint configuration fails assembly, while
collisions with existing business routes retain Axum's normal conflict behavior (panic).

## Middleware scope

`Gotcha::layer` and `GotchaRouter::layer` keep their existing Axum semantics: they cover business
routes already registered, not routes added afterwards or application documentation. If you want
private business routes and public documentation, keep authentication on those business routes.

Use `.app_layer(auth_layer)` on the builder for authentication, CORS, or other middleware that must
cover every application endpoint and fallback. It applies after routes and documentation are
assembled, even when registered before them. Repeated calls wrap in Axum order: the last registered
application layer sees incoming requests first. `with_cors()` installs
`CorsLayer::permissive()` as an application layer too, so it covers the complete application
regardless of the order in which it is called. This changes behaviour compared with the previous
business-route scope:

- Documentation endpoints and `/metrics` now send `Access-Control-Allow-Origin: *`; an `OPTIONS`
  preflight to `/metrics` returns `200` instead of `405`.
- Routes registered after the call are covered: `.get("/public").with_cors().get("/admin")` also
  allows cross-origin requests to `/admin`.
- Application layers keep their usual order (the last one registered runs first). With
  `.app_layer(auth).with_cors()`, or `.with_cors().layer(auth)` for a business layer, the preflight
  is answered with `200` before authentication runs: auth, rate limiting, and audit logging no
  longer observe `OPTIONS` preflight requests. Registering auth with
  `.with_cors().app_layer(auth)` keeps auth outside CORS, as for any other application layer.
- The Prometheus metric layer no longer observes `OPTIONS` preflight requests in any call order,
  because CORS answers them before request metrics are recorded.
- Preflights for paths that match no route already returned `200` on main in three of the four
  call orders; only `.with_cors()` before a custom `.fallback(..)` left them at `404`, and they now
  return `200` in that order too.

To keep the old business-route-only scope, register the routes first and then call
`.layer(CorsLayer::permissive())`; `with_cors()` is now equivalent to
`.app_layer(CorsLayer::permissive())`. As on main, that layer still covers the fallback, so
unmatched paths keep receiving CORS headers.

The corresponding trait hook is
`fn finish_router(&self, router: axum::Router, context: &GotchaContext<Self::State, Self::Config>) -> axum::Router`.
Return `router.layer(auth_layer)` from it to cover the complete application. This hook runs after
documentation is mounted, without requiring an override of `build_router`. An existing custom
`build_router` still owns its whole assembly and must invoke any desired finalization itself.
The builder's `.finish_router(|router, context| ..)` is the equivalent hook; it receives a clone
of the application context (configuration shared behind an `Arc`, state shared behind an `Arc`
still the same value), and `app_layer` middleware remains outermost around whatever the hook mounts.
Routes added through these native Axum hooks do not contribute OpenAPI metadata.

## Export without HTTP

Use `Gotcha::into_openapi()` or `GotchaRouter::into_openapi()` to consume a route definition and
return its `oas::OpenAPIV3`. `GotchaApp::openapi_document()` creates a fresh definition through
`routes()` and consumes that. All three use the same generation and transform pipeline as the
HTTP document; no side channel or HTTP request is needed.

Export does not load configuration, initialize application state, bind a listener, register tasks,
or apply application middleware. User route registration and document transforms still run, so
any side effects written in those callbacks remain. Endpoint paths do not affect the document.
Transforms keep their `FnOnce` captures, nested ordering, and one execution per document.
When documentation is disabled and no export is requested, transforms are not executed at all.

The OpenAPI example demonstrates exporting with `cargo run -p openapi -- --export-openapi`.
It prints JSON and exits without starting the application.

---

# Unreleased: message task handles

`Message` and `Messager` remain available in the same module and root exports, without a feature
flag. They provide a typed command convention and application context, rather than a queue or
transport. `send` still executes directly in the caller's future; its output (including application
errors), panic, and cancellation remain at that boundary.

`Messager::spawn` now returns `tokio::task::JoinHandle<M::Output>` and accepts any message output,
including `Result<T, E>`. Previously it only accepted `Output = ()` and discarded the handle.

```rust,no_run
# use gotcha::{Message, Messager};
# async fn example<M: Message<(), (), Output = Result<u32, std::io::Error>>>(
#     messager: Messager<(), ()>, command: M,
# ) -> Result<(), Box<dyn std::error::Error>> {
let task = messager.spawn(command);
let output = task.await?; // Task panic/cancellation is a JoinError.
let value = output?;     // Application errors stay in the message's Result.
# let _ = value;
# Ok(())
# }
```

Existing calls with a semicolon still compile. Code requiring a `()` return, such as a function
ending in `messager.spawn(command)` without a semicolon or a stored function pointer, must adapt
to the new return type. To explicitly keep the previous detached behavior, use
`drop(messager.spawn(command));`.

Retaining a handle allows joining or aborting; it does **not** enable automatic cancellation on
drop. Dropping it, including when its owning future is cancelled, detaches the task and loses its
result. To cancel, call `task.abort()` and then `task.await` to observe completion. Abort requires
the task to yield and can race with normal completion; already performed side effects remain.

These tasks are not registered with the application's shutdown or scheduler ownership. Await
`messager.send(command)` inside a scheduled execution when the command should share that execution's
lifetime. Commands that spawn further tasks must manage them separately. No queue, retry, or extra
task has been introduced: `spawn` returns the handle from its existing `tokio::spawn` call.

---

# Unreleased: static OpenAPI descriptors

`#[api]` keeps its existing syntax and OpenAPI behavior. Its generated `Operable` now stores
constructor function pointers directly, without `Lazy`, boxed closures, or a runtime constructor
vector. The macro no longer needs UUID-named helper statics or a UUID dependency.

Only code using the low-level descriptor types directly needs to migrate:

| Public type or field | Previous representation | New representation |
| --- | --- | --- |
| `ParamConstructor` | `Box<dyn Fn(String) -> ParamType + Send + Sync>` | `fn(String) -> ParamType` |
| `Operable::parameters` | `&'static Lazy<Vec<ParamConstructor>>` | `&'static [ParamConstructor]` |
| `Operable::responses` | `&'static Lazy<Box<dyn Fn() -> Responses + Send + Sync>>` | `fn() -> Responses` |

Replace lazy constructor collections with a static slice. Replace a lazy boxed response constructor
with a function or a non-capturing closure. For example, these fields can be written directly in a
static `Operable` initializer:

```rust,ignore
parameters: &[<Path<u32> as ParameterProvider>::generate],
extractors: &[<Path<u32> as ParameterProvider>::extractor_kinds],
responses: <String as Responsible>::response,
```

Capturing closures cannot be stored in the new function-pointer fields. Apply configuration-dependent
changes through the existing `.openapi(move |spec| ...)` transform instead. User transforms still
support captured state. `ParameterProvider::generate`, `ParamType`, and the public `gotcha::Either`
re-export retain their existing interfaces.

Static constructor storage does not cache generated parameters, responses, schemas, or complete
OpenAPI documents. Constructors still run inside each document's schema collection scope, preserving
path-specific parameter names, independent documents, generic DTOs, recursive references, and
`errors(...)` / `responses(...)` behavior. This is a structural simplification; no benchmarked
performance improvement is claimed.

---

# Unreleased: owned scheduled tasks

**Task registration no longer starts background work.** The application owns its scheduled tasks
and shuts them down together with HTTP serving.

## Registration and ownership

`TaskScheduler::cron` and `interval` now take `&mut self` and only register work. Existing application
hooks already receive a mutable scheduler. Update helper functions that accepted `&TaskScheduler`
to accept `&mut TaskScheduler`.

All registrations must succeed before serving starts any task. A registration error, panic, or
cancelled startup discards the pending tasks. Do not wait inside the registration hook for a
scheduled task to execute: it cannot start until the hook returns.

Standalone callers must explicitly start the scheduler and retain the returned owner:

```rust,ignore
let mut scheduler = TaskScheduler::new(context);
scheduler.interval("cleanup", Duration::from_secs(60), cleanup);
let running = scheduler.start();
// Keep `running` alive for as long as the tasks should run.
running.shutdown(Duration::from_secs(10)).await;
```

`RunningTasks::shutdown(timeout)` stops subsequent executions, waits for current executions, and
aborts unfinished tasks at the deadline, waiting up to one second for them to stop. Dropping the owner or cancelling its shutdown
future requests immediate abortion; dropping cannot perform an asynchronous join. The same ownership
includes the current execution, which is no longer a separate detached task.

A standalone scheduler uses a child of the context's shutdown token. Cancelling the context token
stops the tasks, while `RunningTasks::shutdown` cancels only the child, so executions waiting on
`context.shutdown()` are released without shutting down the application or another scheduler
sharing the context. Schedulers created by application startup already use the server's token.

## Application shutdown

Builder `run` / `listen` / `listen_on` and `GotchaApp::run` now wait for Ctrl-C or Unix SIGTERM by
default, then use one cancellation signal to stop HTTP acceptance and new scheduled executions.
In-flight HTTP requests drain while scheduled tasks finish. Override the signal with a future:

```rust,no_run
use gotcha::Gotcha;
let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
let app = Gotcha::new().shutdown_signal(async move { let _ = stopped.await; });
// Resolve the future by sending through `stop`; `app.run().await` waits for shutdown to finish.
```

For `GotchaApp`, override `fn shutdown_signal(&self) -> impl Future<Output = ()> + Send` instead.
That future may borrow the application. A custom signal replaces the default OS signal handling.

Scheduled executions get a 30-second grace period by default. Configure it with the builder's
`.task_shutdown_timeout(duration)` or the trait's `fn task_shutdown_timeout(&self) -> Duration`.
The deadline starts when shutdown is requested, independently of HTTP draining. HTTP draining is
bounded separately by `http_shutdown_timeout`; see
[bound servers and shutdown lifecycle](#unreleased-bound-servers-and-shutdown-lifecycle). A serve
error also stops and drains the scheduled tasks; cancelling the serving future requests abortion of
all scheduled work and signals HTTP shutdown.

Cancellation requires async tasks to yield. A task blocking its thread never observes the abort:
one second after it, shutdown names the task in a warning and continues without it, and the runtime
waits for that thread when it is dropped. Cleanup hooks can overlap a `Drop` that takes longer than
that second. A task that blocks right after a timer woke it can stall the runtime itself, so that
shutdown does not start until it stops blocking; move blocking work to
`tokio::task::spawn_blocking`. Tasks explicitly detached by application code (including `Messager::spawn`) remain outside this ownership.

Single-execution panics are logged and the schedule continues, including a panic while constructing
the execution future. Interval tasks retain their existing immediate first tick and non-overlapping
executions. A slow execution no longer causes catch-up runs: an interval execution runs the
already-due tick once when it finishes and then resumes at the next period boundary
(tokio's `MissedTickBehavior::Skip`), and cron tasks compute the next trigger from the current
time, anchored to the last trigger that fired so a wall-clock step back cannot repeat it. The
low-level `cron_proc_macro_wrapper` and `interval_proc_macro_wrapper` drivers share these rules.
Invalid cron expressions and zero intervals are registration errors that fail application startup;
see [invalid task registrations fail startup](#unreleased-invalid-task-registrations-fail-startup).

---

# Unreleased: shared startup

**Builder and trait applications now share configuration policy, address resolution, binding,
initialization, and serving.** This changes the builder's default error policy and listener source.

## Listener address

Previously, builder `run()` selected its address before loading configuration and ignored
`config.server`. It now applies this priority:

| Priority | Source |
| --- | --- |
| 1 | Explicit `listen(...)` / `listen_on(...)` socket address |
| 2 | Builder `.host(...)` and `.port(...)`, independently |
| 3 | Loaded or explicitly supplied `ConfigWrapper::server` |
| 4 | Framework defaults from a missing server section |

An explicit `.port(3000)` overrides a configured 8080, even though 3000 is the framework default.
Setting only the port preserves the configured host. `run()` supports IPv4 and IPv6 literals;
use an unbracketed IP in `server.host` and a bracketed IPv6 socket address for `listen()`.
Hostname resolution is not provided.

Applications relying on the old builder behavior should explicitly call `.host("127.0.0.1")`
and `.port(3000)`, or pass an explicit listen address. `Gotcha::from_context` now honors its supplied
server settings as well.

## Configuration errors

Automatic loading and explicitly registered sources now both fail startup on loading errors unless
a fallback is explicitly selected. Optional files still ignore only missing files. Explicitly supplied
configuration still takes precedence over sources. To opt into fallback in the builder:

```rust
use gotcha::{ConfigErrorPolicy, Gotcha};
let app = Gotcha::new().config_error_policy(ConfigErrorPolicy::fallback_to_default());
```

For the trait API, add this method to your `GotchaApp` implementation when its config supports Default:

```rust,ignore
fn config_error_policy(&self) -> gotcha::ConfigErrorPolicy<Self::Config> {
    gotcha::ConfigErrorPolicy::fallback_to_default()
}
```

`ConfigErrorPolicy::Fallback(factory)` accepts a function returning `ConfigWrapper<C>` for custom
fallback values. Fallback logs the original error, replaces the whole configuration, and then applies
the normal address overrides. It applies to the configuration-loading step of startup, including a
custom trait `config()` error; it does not swallow address, state, router, task, or serve errors.
Eager `.build_config(...)` and direct `GotchaApp::config()` calls keep returning their errors.

`with_config::<C>()` and the config parameter of `with_types::<S, C>()` no longer require `Default`.
Only selecting `fallback_to_default()` needs `C: Default`; state default construction retains its
existing bound. Initialized contexts still require no serde or Default implementations.

## Initialization order

After the existing logging hook, startup loads configuration, selects and binds the address,
initializes state, assembles the router, registers tasks, and serves. This order is identical for
both APIs. A bind failure occurs before state construction or task registration; a later startup
error releases the listener. The initialized context's server settings contain the actual bound
address, including a dynamically assigned port, so state initialization and handlers agree.

State initialization now runs while the port is reserved, before requests are accepted. Applications
that depended on initializing state before a bind attempt must account for the new order. See
[owned scheduled tasks](#unreleased-owned-scheduled-tasks) for task startup and shutdown behavior.

---

# Unreleased: runtime type bounds

Initialized values no longer have to implement loading or default-construction traits just to
be stored in a context, extracted by handlers, or used by messages and scheduled tasks.

- Use `Gotcha::from_state(value)` for state without `Default` and automatic empty configuration.
- Use `Gotcha::from_context(GotchaContext::new(state, config))` when both values are already ready.
  State and application configuration only need `Clone + Send + Sync + 'static` for runtime use.
- `with_state::<S>()`, `with_config::<C>()`, and `with_types::<S, C>()` retain automatic initialization.
  Default state construction requires `S: Default`; configuration loading requires `Deserialize`.
  Creating state defaults and reading files still happen at startup, after any explicit overrides.
- `.state(...)`, `.config(...)`, route composition, task registration, and serving no longer carry
  unrelated `Default` or serde requirements. `.build_config(...)` requires `C: DeserializeOwned`.

`ConfigWrapper<T>` and `GotchaContext<S, C>` impose no bounds at their type definitions.
`ConfigWrapper`'s derived implementations request `Serialize`, `Deserialize`, `Clone`, or `Default`
only for the corresponding operation. `#[state]`, `#[config]`, and `FromRef` extraction no longer
impose loading bounds on the stored configuration or unrelated state type.

For the trait API, `GotchaApp::Config` only requires the runtime bounds. `build_router(context)`
can therefore accept an explicit configuration without serde or `Default`. `config()` and `run()`
require `Self::Config: DeserializeOwned`, because their contract includes configuration loading.
This method bound applies even when `config()` is overridden; use an explicit context if the
configuration cannot be deserialized. Generic callers of these methods must now state that bound
explicitly. `Serialize` and `Default` are no longer required by trait-based loading.

Explicit configuration still wins over registered sources. See [shared startup](#unreleased-shared-startup)
for the unified strict loading policy, explicit fallback, and listener address precedence.

See the [initialized-values example](README.md#initialized-state-and-configuration).

---

# Unreleased: bound servers and shutdown lifecycle

## Bind before serving

`Gotcha::bind()` / `Gotcha::bind_on(addr)` and `GotchaApp::bind()` initialize the application
(configuration, state, routes, tasks) and bind the listener, then return a public `BoundServer`
instead of serving. `BoundServer::local_addr()` reports the effective address, including the port
selected for port `0`, and `BoundServer::serve()` runs the server. `run()`, `listen()`, and
`listen_on()` keep their signatures and are now thin wrappers over `bind*().serve()`.

`GotchaApp::bind()` requires `Self: 'static` because the bound server erases the application into
its signal and cleanup driver. Applications that borrow external data can keep using `run()`.

Signal handling starts only when serving starts. An unserved `BoundServer` does not install a
signal handler, so holding one does not swallow Ctrl-C/SIGTERM. Dropping one cancels the context
shutdown token (because routes or `finish_router` hooks may have captured it), releases the
listener, and runs no cleanup.

`BoundServer::serve_with_shutdown(future)` replaces the application signal completely: the
configured `shutdown_signal` is not polled, so Ctrl-C and SIGTERM do not stop that server. Cleanup
hooks still run after it drains.

## Context shutdown token

`GotchaContext` now carries a shutdown token, available through `context.shutdown()`. It is
cancelled when the shutdown signal resolves and before HTTP draining starts, so handlers can select
on it to close WebSockets, server-sent events, and other long-lived responses at the moment the
listener stops accepting. Cancelling the token also requests shutdown.

`GotchaContext` can no longer be constructed with struct literal syntax. Replace:

```rust,ignore
GotchaContext { state, config }
```

with:

```rust,ignore
GotchaContext::new(state, config)
```

Exhaustive destructuring (`let GotchaContext { state, config } = context;`) also stops compiling;
read `context.state` / `context.config` directly instead. A context passed to
`Gotcha::from_context` has its own token discarded; the running server owns the token handlers and
cleanup hooks see.

`gotcha::CancellationToken` is a re-export of `tokio_util::sync::CancellationToken`, so
**`tokio-util` 0.7 is now part of gotcha's public API**. Naming the type directly requires a
compatible `tokio-util` dependency.

## Bounded HTTP draining

HTTP draining was previously unbounded. `http_shutdown_timeout` now defaults to 30 seconds
(builder `.http_shutdown_timeout(duration)` or trait `http_shutdown_timeout(&self)`). Upgraded
connections (WebSockets) are no longer tracked by axum's graceful shutdown after the upgrade, so
the drain also waits for those tracked streams. At the deadline, every tracked socket is shut down
at the OS level (so handlers blocked off-socket are disconnected too), handlers get up to one
second to observe the closure, and shutdown continues even if some remain. Scheduled tasks keep
their independently configured `task_shutdown_timeout` and drain concurrently with HTTP.

## Cleanup hooks

Register ordered cleanup with builder `.on_shutdown(|context| async move { ... })` (called in
registration order) or override `GotchaApp::on_shutdown(&self, context)`. Hooks receive the
application context and run after serving stops. Errors are not returned; log inside the hook.

A panicking hook does not prevent later hooks from running; the first panic is re-raised from
`run()` / `serve()` after all hooks have run. A hook that never resolves blocks shutdown, because
application cleanup has no timeout.

The complete order is:

1. the shutdown signal resolves and the context token is cancelled,
2. the listener stops accepting,
3. in-flight HTTP connections, including upgraded WebSockets, drain (bounded by
   `http_shutdown_timeout`),
4. scheduled tasks stop starting and drain (bounded by `task_shutdown_timeout`,
   concurrently with HTTP draining),
5. cleanup hooks run in order, and only then do `run()` / `serve()` return.

A failed bind or initialization runs no hooks. Dropping a `BoundServer` without serving it, or
cancelling the serving future, releases the listener and skips cleanup.

---

# Unreleased: HTTP response contracts

**`Schematic` no longer implies `Responsible`.** A data schema does not determine how a value is
sent over HTTP. For automatic response inference, custom HTTP types implement `Responsible`
separately, alongside `IntoResponse`; they can still derive `Schematic`.
Ordinary JSON data should be returned as `Json<T>`.
The HTTP-specific `Schematic::empty_body` hook has been removed; `()` has its own response impl.

Built-in contracts now match the response representation:

| Return type | Documented response |
| --- | --- |
| `String`, `&str`, boxed/Cow strings | `200 text/plain` |
| `Json<T>` | `200 application/json`, using `T`'s schema |
| `Html<T>` | `200 text/html` |
| `Bytes`, `Vec<u8>`, byte slices, boxed/Cow bytes | `200 application/octet-stream`, binary string schema |
| `()` | `200`, no body; `Json<()>` remains JSON |
| `(StatusCode, T)` | `default`, with `T`'s response bodies; the status is a runtime value |
| `StatusCode` / raw `Response` | `default`, with no body schema inferred |

Media keys omit parameters such as `charset=utf-8`. Regenerate client/spec snapshots that
previously described text or binary responses as JSON.

Use `WithStatus<T, STATUS>` to keep a fixed status in the return type and the actual response:

```rust
use gotcha::{Json, WithStatus};

let created: WithStatus<_, 201> = WithStatus::new(Json("created"));
let no_content: WithStatus<_, 204> = WithStatus::new(());
```

This wrapper is available without the `openapi` feature. Statuses outside 100..=599 fail during
compilation when constructed. Informational statuses, 204, 205, and 304 discard bodies and body
headers. As with Axum's status tuple, the outer status overrides an inner error status too;
prefer `Result<WithStatus<Json<User>, 201>, E>` to keep `E`'s status independent.

**Automatic inference for `Result<T, E>` requires `Responsible` on both sides.** `ErrorResponsible`
and its schema blanket impl have been removed. Migrate custom errors to `Responsible`, use a fixed
HTTP type such as `WithStatus<Json<ApiError>, 409>`, or declare the endpoint's complete error contract
with `errors(...)` as shown below. A schema-only error no longer automatically adds `default`.
To retain a dynamic JSON error contract, implement `Responsible` using
`gotcha::response::default_response::<ApiError>("application/json", "Error")`.

For runtime status tuples or additional responses, declare the contract on the handler:

```rust,ignore
#[api(
    responses(
        response(status = 404, body = "ApiError", description = "Not found"),
        response(status = 409, body = "ApiError", description = "Duplicate")
    ),
    drop_default
)]
async fn lookup() -> Result<Json<User>, (StatusCode, Json<ApiError>)> {
    // Return the documented statuses here.
}
```

`body` names a **data type implementing `Schematic`**, not `Json<T>` or another HTTP wrapper.
`content_type` defaults to `application/json` when a body is present; set it explicitly for CSV
or other media. Omit `body` for an empty response. Duplicate statuses, invalid body types and
body declarations on bodyless statuses are rejected by the macro.

Explicit entries replace inference for their status only. `drop_default` removes the inferred
default, and at least one response must remain. These declarations change documentation only;
the handler remains responsible for sending the declared status and content type. Declared body
schemas participate in the same component collection and reference rewriting as inferred bodies.

When the return type cannot be inferred — WebSocket upgrades, SSE streams, or any opaque type
such as `Sse<impl Stream<...>>` — add `skip_response`. The nonempty `responses(...)` list then
describes the return-type part of the contract, and the return type is never placed in a trait
path. Extractor rejection responses are still added; see
[extractor rejection responses in OpenAPI](#unreleased-extractor-rejection-responses-in-openapi):

```rust,ignore
#[derive(Schematic)]
struct ChatSseEvent {
    data: String,
    event: Option<String>,
    id: Option<String>,
    retry: Option<u32>,
}

#[api(
    skip_response,
    responses(response(
        status = 200,
        content_type = "text/event-stream",
        item = "ChatSseEvent"
    ))
)]
async fn events() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Return the stream here.
}
```

`item` requires an explicit `content_type` and sets OpenAPI 3.2's `itemSchema`, the schema of
each parsed item in a sequential media type; `body` remains the independent whole-body schema.
For `text/event-stream`, the item schema must describe the **parsed event envelope** — `data`
plus the optional `event`, `id`, and `retry` fields — not only the JSON carried inside `data`.
A hand-written `Schematic` can attach `contentMediaType: application/json` and `contentSchema`
to `data` when that JSON has its own schema. For `application/jsonl`, `item` describes each
JSON value instead.
`skip_response` cannot be combined with `errors(...)` or `drop_default`: with no inference there
is no inferred default to drop. A WebSocket route declares its upgrade as an empty response with
`skip_response, responses(response(status = 101))`. Declared item types join the same component
collection as inferred bodies.

Parameter-level `#[api(skip)]` is now parsed structurally: it accepts exactly `#[api(skip)]`, and
unknown `#[api(...)]` contents on a parameter are compile errors instead of being matched by
substring.

For a `thiserror` enum that already implements `IntoResponse`, use `errors(...)` to describe all
errors for this endpoint without implementing `Responsible` on the enum:

```rust,ignore
#[api(errors(
    response(status = 404, body = "ErrorBody", description = "Not found"),
    response(status = 409, body = "ErrorBody", description = "Duplicate"),
    response(status = 500, body = "ErrorBody", description = "Internal error")
))]
async fn create_user() -> Result<Json<User>, ApiError> {
    // ApiError::into_response() sends the actual status and ErrorBody.
}
```

`ErrorBody` is the serialized data type and must implement `Schematic`; `ApiError` itself needs
neither `Schematic` nor `Responsible`. The success type still needs `Responsible`. Type aliases
such as `type ApiResult<T> = Result<T, ApiError>` work as well. See the
[README example](README.md#openapi-documentation) for a complete `thiserror` implementation.

`errors(...)` requires a `Result` return type and at least one response, using the same declaration
fields and validation as `responses(...)`. It completely replaces inference of the error branch,
even if the error type implements `Responsible`. Another endpoint using the same enum can declare
a different error set. The success contract is kept in full, including any dynamic `default`;
`drop_default` is not needed to suppress error inference.

Assembly order is: infer the success branch, merge the declared error alternatives, apply
`responses(...)` overrides, then apply an explicit `drop_default`. If success and error branches
share a status, their media types and schemas are combined using the same rules as ordinary
`Result` inference. With no `errors(...)`, existing inference and `responses(...)` behavior are
unchanged. None of these attributes changes runtime HTTP behavior.

Public helpers in `gotcha::response` support custom contracts: `response::<T>(status, media,
description)`, `empty_response(status, description)`, `default_response::<T>(media, description)`,
and `merge_responses(&mut target, other)`. The latter is also used by `Result`: duplicate statuses
retain media alternatives, differing schemas for one media type form `anyOf`, and identical
definitions are reused. Later header/link/example keys win. Distinct response-level `$ref`s
cannot be unioned and fail at assembly; use inline responses with schema-level references instead.

---

# Unreleased: application and route composition

**`Gotcha::nest` and `Gotcha::merge` now accept `GotchaRouter<GotchaContext<S, C>>`, not another
`Gotcha<S, C>`.** Previously they extracted only the child's router and silently discarded its
state, configuration and sources, listening address, and background task registrations.

Replace route-only child builders with route modules:

```rust
use gotcha::{Gotcha, GotchaRouter};

let api = GotchaRouter::default().get("/items", || async { "items" });
let health = GotchaRouter::default().get("/health", || async { "ok" });
let app = Gotcha::new().nest("/api", api).merge(health);
```

Route modules share the application's context type and receive its resolved state and config.
They retain their routes, middleware, and OpenAPI metadata/transforms. The existing nested path
and child-before-parent transform rules continue to apply.

Move child `.state(...)`, `.config(...)`, configuration-source calls, `.host(...)`, `.port(...)`,
and `.tasks(...)` registrations to the top-level application deliberately. If multiple modules
need tasks, register each module's task setup with that application's `.tasks(...)`; route modules
do not own a scheduler. There is no automatic merge of competing application settings or tasks,
and no conversion that extracts routes while discarding the rest of an application.

Full child applications now fail to compile at `nest`/`merge`, including children with task
registrations. Applications that need independent state/configuration and lifecycles should be
started independently. To reuse a trait application's `routes` method, supply a router with the
same context type; doing so reuses only routes, not its `state`, `tasks`, or startup hooks.

---

# Unreleased: documented method routers

**`Gotcha::route` and `GotchaRouter::route` now require `MethodRouter`.** This type carries
HTTP handlers and their OpenAPI descriptors together. Previously, passing an Axum `MethodRouter`
silently omitted even `#[api]` handlers from the generated document.

For documented composition, use Gotcha's method constructors:

```rust
use gotcha::{routing::get, GotchaRouter};

// With the openapi feature, annotate these handlers with #[gotcha::api].
async fn list() -> String { "items".into() }
async fn create() -> String { "created".into() }

let router: GotchaRouter<()> = GotchaRouter::default()
    .route("/items", get(list).post(create));
```

`gotcha::get/post/...` and the corresponding prelude exports now return this Gotcha type too.
When using a plain Axum `Router`, import constructors from `axum::routing` explicitly.
Existing `.get(path, handler)` / `.post(path, handler)` shortcuts keep their signatures and use
the same registration path as `.route(...)`.

To preserve native Axum behavior, change `.route(...)` to `.route_raw(...)` on either Gotcha API:

```rust
use gotcha::{axum, GotchaRouter};

let router: GotchaRouter<()> = GotchaRouter::default()
    .route_raw("/health", axum::routing::get(|| async { "ok" }));
```

Raw routes generate no operations, including for annotated handlers. No implicit conversion
between Axum and Gotcha method routers is provided. Use raw routes for native services or other
Axum-specific method-router APIs.

Gotcha method routers support method chaining, `on`, `merge`, `clone`, and `layer`, retaining
their descriptors through each operation. Components are still collected once for the complete
document. Unannotated handlers run normally and remain undocumented; middleware does not infer
response schemas or security requirements. Overlapping handlers follow Axum's conflict rules.

Combined filters such as `routing::on(MethodFilter::GET.or(MethodFilter::POST), handler)` now
document every selected OpenAPI method. Implicit HEAD handling for GET is unchanged and does
not add a HEAD operation; explicitly register HEAD to document it. CONNECT remains executable
but is not represented by the current oas PathItem model.

---

# Unreleased: OpenAPI transform composition

Repeated `.openapi(...)` calls now append callbacks instead of replacing the previous callback.
`nest` and `merge` retain child callbacks instead of silently dropping them. Both `Gotcha` and
`GotchaRouter` use these rules:

1. Child subtrees run in `nest`/`merge` insertion order, with descendants before their parent.
2. The current router's own callbacks then run in registration order, even if registered before
   its children. The receiver of `merge` is the parent; regrouping merges can change precedence.
3. Each callback runs once during router assembly, on the complete document with final prefixed
   paths and collected components. Requests to `/openapi.json` reuse that result.

**Previously discarded callbacks now execute.** Review callbacks that overwrite metadata or have
side effects. Parent callbacks can explicitly override child edits. Later assignments win; there
is no implicit merge of `info`, component maps, or security lists. Mutate individual map entries
to preserve unrelated definitions, or explicitly replace a field to reset it.

**Child callbacks configure the whole document.** Top-level `security` applies globally, including
routes outside that child. For local requirements, edit `security` on specific operations using
their final prefixed paths. Moving a callback into a nested router does not make it route-scoped.

---

# Unreleased: nested OpenAPI paths

`GotchaRouter::nest` now records OpenAPI paths using the same join rules as the underlying
Axum router. For example, nesting `/hello` under `/api` documents `/api/hello`, rather than the
incorrect `/api//hello`. Regenerate clients/spec snapshots that previously contained these
incorrect paths; the application's actual HTTP routes and registration API are unchanged.

A child root `/` documents `/api` when nested under `/api`, and `/api/` when nested under
`/api/`. Trailing slashes on non-root child routes and intentional interior double slashes
remain significant. The same rules apply at each level of nesting and preserve path parameters.

---

# Unreleased: schema identity and component names

Schema collection now identifies derived types by their full Rust type name, including generic
arguments, instead of treating the display name as the type's identity. Different types no longer
silently share the first schema encountered. Operations are assembled in path/method order;
component names are resolved after collection, independently of registration order.

| Rust types in one document | Component names |
| --- | --- |
| A unique `User` | `User` |
| `Envelope<String>`, `Envelope<u32>` | `Envelope_String`, `Envelope_u32` |
| `screens::SendResult`, `terminals::SendResult` | `ScreensSendResult`, `TerminalsSendResult` |
| `first::models::Item`, `second::models::Item` | `FirstModelsItem`, `SecondModelsItem` |

All generated references, including recursive and nested references, follow the resolved names.
Each name collision emits a `tracing::warn!` with `schema_name`, `types`, and `resolved_names`.
Repeated use of the same type is normal reuse and does not warn. Add a tracing subscriber to
observe these events, as for other framework logs.

**Component names and generated client type names may change.** Unique short names remain as
before, but adding a colliding type can rename a previously unique component. Reserve stable
public names explicitly:

```rust
use gotcha::Schematic;

#[derive(Schematic)]
#[schematic(name = "ScreenSendResult")]
struct SendResult { screen_id: String }
```

The override works for structs, all supported enum representations, and newtypes. A named newtype
becomes its own component; an unnamed newtype remains transparent. An explicit override also
changes `Schematic::name()`. Names must match `[A-Za-z0-9._-]+`.

A unique explicit name takes priority over an automatically generated name. Duplicate explicit
names warn and are disambiguated, rather than dropping either schema. Prefer unique overrides,
including across generic instances that share a derive. If module prefixes still collide (for
example when generic arguments have identical short names), an encoded full type identity is
used as a deterministic fallback. Use explicit names for such public types rather than relying
on these long fallback names. Automatic identity/naming is not a cross-compiler ABI; explicit
unique names are the mechanism for pinning a public contract.

**Low-level `registry::collect` results now require `SchemaReferences`.** The collector must
rewrite references returned by the closure as well as references in component definitions after
assigning names. Standard schema/operation results, vectors, maps, and parameter-provider results
already implement this trait. Custom result containers should forward it to their schema-bearing
fields; unrelated application state does not need serialization or new bounds. Temporary
references inside the closure are not finalized until `collect` returns.

Handwritten schema implementations can use `registry::schema_or_ref_for::<Self>` with an optional
explicit name and `module_path!()` to participate in type-aware collection. The older
`schema_or_ref(name, ...)` helper still treats its caller-supplied name as an explicit identity;
callers of that helper remain responsible for making those identities unique.

---

# Unreleased: ordered configuration sources

Configuration sources now retain their insertion order when cloned or restored. Later sources
override earlier matching values, including when files and environment prefixes are interleaved:

```rust
use gotcha::config::ConfigBuilder;

let builder = ConfigBuilder::new()
    .file("base.toml")
    .env("APP")
    .file_optional("local.toml");
let restored = ConfigBuilder::from_state(builder.state());
```

Here `APP` overrides `base.toml`, and `local.toml` overrides both. Previously restoration grouped
all environment sources before all files, changing precedence. `Gotcha::with_file_config`,
`with_optional_config`, `with_env_config`, and the default-source helpers use this same ordering.
To let environment values win, add their source after the files.

**Direct `ConfigState` construction is a source-breaking change.** Replace `file_paths` and
`env_prefixes` with the ordered `sources` list, specifying which files are required:

```rust
use gotcha::config::{ConfigSource, ConfigState};

let state = ConfigState {
    sources: vec![
        ConfigSource::File { path: "base.toml".into(), required: true },
        ConfigSource::Env { prefix: "APP".into() },
        ConfigSource::File { path: "local.toml".into(), required: false },
    ],
    enable_vars: true,
};
```

The chainable builder methods keep their signatures. State is a description of where to load
configuration, not a snapshot: files and environment values are read at `build()` time. A file
created after registration is now included, and a required file removed before loading fails.
An optional file only ignores `NotFound`; invalid TOML, invalid UTF-8 and other read errors fail.

**Explicit configuration-source failures now stop Builder startup.** Calls such as
`with_file_config`, `with_optional_config`, and `with_default_config` propagate loading errors
through `run()` / `listen()` instead of replacing the configuration with `Default`. Use
`with_optional_config` for a file whose absence is acceptable. The conventional files selected
by `with_default_files` are still optional, but errors in existing files propagate.

An already supplied `.config(...)` value still takes precedence over accumulated sources.
Automatic loading now follows the same strict default policy; see [shared startup](#unreleased-shared-startup)
for explicitly opting into fallback.

---

# Unreleased: typed configuration defaults

Framework-managed loading now installs the reserved `[server]` settings as typed defaults beneath
files and environment variables. `APP_SERVER__PORT=9000` therefore loads on a machine with no
`configurations/application.toml` instead of failing with
`invalid type: string "9000", expected u16`. Because the defaults provide real TOML types, string
values for those fields are coerced rather than rejected.

- `ConfigSource` gained a `Defaults { value }` variant. Add a wildcard arm to exhaustive matches.
- `ConfigBuilder::framework_defaults()` creates a builder with the framework's typed server
  defaults beneath every other source. Framework-managed loading starts there; custom
  `GotchaApp::config()` implementations should too, or environment-only `APP_SERVER__PORT`
  will not coerce.
- `ConfigBuilder::defaults(value)` registers application defaults at the same layer and returns
  `ConfigResult<Self>` because the value must be representable as TOML. Application defaults
  override framework defaults, while files and environment variables override both. Use it from
  `Gotcha::build_config` or a custom `GotchaApp::config()`.
- `Gotcha::build_config` starts its builder function above the framework defaults. Automatic
  loading (`GotchaApp::config`'s default and the builder's automatic stack) and every `with_*`
  source helper use the same defaults layer.

---

# Unreleased: default configuration sources

`Gotcha::with_default_config()`, `with_default_files()`, and `with_default_env()` now enable
variable substitution, matching the explicit `with_*` helpers and automatic loading. `${VAR}`
and path placeholders in the conventional files and the `APP` environment prefix are resolved
instead of being passed through literally. A variable or path that is not set resolves to an
empty string; it does not fail the load.

`Config::load_default` now returns `ConfigResult<T>` and no longer falls back to `T::default()`
when loading fails. Propagate the error, or choose an explicit fallback with
`ConfigErrorPolicy::fallback_to_default` during startup.

---

# 0.3 → 0.4

Two changes require edits in every application: **route paths** and the **configuration file**. Everything else is a smaller adjustment.

## 1. Route paths use `{name}`, not `:name`

Gotcha now builds on axum 0.8, which changed how a captured path segment is written — and rejects the old form outright rather than silently treating it as a literal:

```rust,ignore
// before                                  // after
router.get("/users/:id", get_user)         router.get("/users/{id}", get_user)
router.get("/f/*rest", serve)              router.get("/f/{*rest}", serve)
```

A path that still starts a segment with `:` fails at startup with *"Path segments must not start with `:`. For capture groups, use `{capture}`"*.

This is the syntax OpenAPI already used, so gotcha no longer translates between the two — a route is documented exactly as it is registered.

## 2. Configuration files: application settings move to the top level

The framework's own settings now live in a reserved `[server]` section, and your application's settings are the top level of the file — they used to be nested under `[application]` while `[basic]` took the top spot.

```toml
# before (0.3)                    # after (0.4)
[basic]                           name = "my-app"
host = "127.0.0.1"                database_url = "postgres://localhost/app"
port = 8080
                                  [server]
[application]                     host = "127.0.0.1"
name = "my-app"                   port = 8080
database_url = "postgres://..."
```

Both profile files (`application.toml` and `application_{profile}.toml`) need the same treatment.

## 3. Reading configuration in code

```rust,ignore
// before                         // after
config.application.name           config.name
config.basic.port                 config.server.port
```

`ConfigWrapper<T>` now dereferences to your own config type, which is what makes `config.name` work.

Handlers can skip the wrapper entirely. Annotate your config type with `#[config]` and extract it directly:

```rust,ignore
#[config]
#[derive(Clone, Default, Serialize, Deserialize)]
struct Config {
    name: String,
}

async fn handler(State(config): State<Config>) -> impl Responder {
    config.name.clone()
}
```

The bind settings are their own extractor, `State<ServerConfig>`. `State<ConfigWrapper<Config>>` still works if you want both at once.

## 4. Environment overrides use `__` between sections

Nested paths are separated by a **double** underscore, which leaves single underscores free for snake_case field names:

```console
# before (never actually worked for typed fields — it failed the whole load)
APP_SERVER_PORT=8080

# after
APP_SERVER__PORT=8080     # -> [server] port
APP_DATABASE_URL=...      # -> the top-level `database_url` field
```

Typed fields (numbers, booleans) are now parsed from the environment string instead of failing to merge.

## 5. The `message` feature is gone

The message system is always available. Drop it from your feature list:

```toml
# before
gotcha = { version = "0.3", features = ["openapi", "message"] }
# after
gotcha = { version = "0.4", features = ["openapi"] }
```

`cors` and `static_files` keep their names, but each now enables only its own half of `tower-http` — a CORS-only application no longer compiles the static-file stack.

## 6. Smaller changes

- **Validation rejections return `422`**, not `400`. `400` is still used for a malformed body. Every error now carries a readable `message`.
- **`Result<T, E>` handlers** require `Responsible` on both sides for automatic inference, or can use `#[api(errors(...))]` for explicit error documentation; see [HTTP response contracts](#unreleased-http-response-contracts) for migration.
- **Handlers returning nothing** now compile (they previously failed with `E0782`) and document an empty body.
- **`Operable`** gained `summary` and `security` fields; only relevant if you construct it by hand rather than through `#[api]`.
- **axum 0.8** also removed `#[async_trait]` from its extractor traits. A hand-written `FromRequest` / `FromRequestParts` impl should drop the attribute and use a plain `async fn`.
- **New re-exports**, so these no longer need `gotcha::axum::…`: `Form`, `Multipart`, `Sse` / `Event` / `KeepAlive`, `WebSocketUpgrade` / `WebSocket`, `middleware`, `MatchedPath`, `OriginalUri`. `GotchaRouter` also gained `fallback_service`.

---

# 0.2 → 0.3: API simplification

This section helps you migrate from the traditional trait-based API to the simplified builder API introduced in Gotcha v0.3.0. Note that the configuration examples below use the 0.3 layout — see the 0.4 section above for the current one.

## TL;DR

- **New projects**: Use the new `gotcha::prelude::*` and builder API
- **Existing projects**: Continue working without changes, migrate at your own pace
- **Both APIs**: Can be used together in the same project

## Overview of Changes

### New Builder API Benefits

✅ **Simplified setup** - No struct definitions or trait implementations required
✅ **Inline handlers** - Define handlers as closures directly in route definitions  
✅ **Fluent interface** - Chain method calls for readable code
✅ **Reduced boilerplate** - 90% less code for simple applications
✅ **Better beginner experience** - Start building APIs immediately
✅ **Full backward compatibility** - Existing code continues to work

## Migration Examples

### Example 1: Simple Hello World

#### Before (v0.2.x)
```rust
use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, State, Responder};
use serde::{Deserialize, Serialize};

pub async fn hello_world(_state: State<ConfigWrapper<Config>>) -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    pub name: String,
}

pub struct App {}

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) 
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router.get("/", hello_world)
    }

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) 
        -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
```

#### After (v0.3.x)
```rust
use gotcha::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::new()
        .get("/", || async { "hello world" })
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}
```

**Lines of code**: 35 → 8 (77% reduction)

### Example 2: JSON API with Path Parameters

#### Before
```rust
use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Json, Path, State, Responder};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct User {
    id: u32,
    name: String,
}

pub async fn get_user(Path(id): Path<u32>) -> impl Responder {
    Json(User { id, name: format!("User {}", id) })
}

pub async fn create_user(Json(user): Json<User>) -> impl Responder {
    Json(user)
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {}

pub struct App {}

impl GotchaApp for App {
    type State = ();
    type Config = Config;

    fn routes(&self, router: GotchaRouter<GotchaContext<Self::State, Self::Config>>) 
        -> GotchaRouter<GotchaContext<Self::State, Self::Config>> {
        router
            .get("/users/{id}", get_user)
            .post("/users", create_user)
    }

    async fn state(&self, _config: &ConfigWrapper<Self::Config>) 
        -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
```

#### After
```rust
use gotcha::prelude::*;

#[derive(Serialize, Deserialize)]
pub struct User {
    id: u32,
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Gotcha::new()
        .get("/users/{id}", |Path(id): Path<u32>| async move {
            Json(User { id, name: format!("User {}", id) })
        })
        .post("/users", |Json(user): Json<User>| async move {
            Json(user)
        })
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}
```

### Example 3: Reusing Route Modules

Extract reusable routes into a function whose context type matches the hosting application:

```rust
use gotcha::prelude::*;

type Context = GotchaContext<EmptyState, EmptyConfig>;

fn api_routes() -> GotchaRouter<Context> {
    GotchaRouter::default().get("/items", || async { "items" })
}

let app = Gotcha::new()
    .get("/health", || async { "ok" })
    .nest("/api/v1", api_routes());
```

A trait-based application with the same context type can also include `api_routes()` in its
`routes` method. In either case, the hosting application provides state, configuration and tasks;
the route module is not an independently initialized application.

## Migration Strategies

### Strategy 1: Fresh Start (Recommended for New Projects)
- Start new projects with `use gotcha::prelude::*`
- Use builder API for all new code
- Reference the `/examples/simple/` for patterns

### Strategy 2: Gradual Migration (Existing Projects)
1. **Keep existing code** - No changes needed immediately
2. **Add new routes** using builder API when convenient
3. **Refactor incrementally** during feature updates
4. **No rush** - both APIs will be supported long-term

### Strategy 3: Side-by-Side (Large Projects)
- Use trait API for complex features (state management, tasks, etc.)
- Use builder API for simple endpoints and utilities
- Mix approaches based on complexity needs

## Feature Comparison

| Feature | Trait API | Builder API | Notes |
|---------|-----------|-------------|-------|
| Simple routes | ❌ Complex | ✅ Easy | Builder API much simpler |
| State management | ✅ Full support | ⚠️ Basic | Trait API better for complex state |
| Configuration | ✅ Full control | ✅ Smart defaults | Both supported |
| Task scheduling | ✅ Integrated | ❌ Not available | Use trait API for background tasks |
| Middleware | ✅ Full control | ✅ Simplified | Both approaches work |
| Testing | ✅ Full control | ✅ Simplified | Builder API easier to test |
| OpenAPI | ✅ Full support | ✅ Auto-enabled | Both generate documentation |

## When to Use Which API

### Use Builder API When:
- ✅ Creating simple web services or APIs
- ✅ Prototyping or learning
- ✅ Most routes don't need complex state
- ✅ You want minimal boilerplate
- ✅ Building REST APIs with standard patterns

### Use Trait API When:
- ✅ Complex application state management needed
- ✅ Background task scheduling required
- ✅ Custom configuration loading logic
- ✅ Advanced lifecycle hooks needed
- ✅ Large applications with multiple modules

### Use Both When:
- ✅ Migrating existing applications
- ✅ Different complexity needs in the same app
- ✅ Team has mixed experience levels

## Import Changes

### Before
```rust
use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Json, Path, State, Responder};
use serde::{Deserialize, Serialize};
```

### After
```rust
use gotcha::prelude::*;
// This includes all commonly used types:
// Gotcha, Json, Path, State, Responder, StatusCode, etc.
```

## Configuration Changes

### Simple Configuration (New)
```rust
Gotcha::new()
    .host("0.0.0.0")
    .port(8080)
    .with_cors()
    .with_openapi()
```

### Advanced Configuration (Existing)
```rust
// Still works exactly the same
impl GotchaApp for App {
    async fn config(&self) -> Result<ConfigWrapper<Self::Config>, Box<dyn std::error::Error>> {
        // Custom config loading
    }
}
```

## Common Patterns

### Error Handling
```rust
// Simple error responses
.get("/might-fail", || async {
    if some_condition {
        Ok("Success")
    } else {
        Err("Something went wrong")
    }
})

// Custom status codes
.get("/not-found", || async {
    (StatusCode::NOT_FOUND, "Resource not found")
})
```

### JSON Responses
```rust
// Simple JSON
.get("/json", || async {
    Json(json!({"message": "Hello"}))
})

// Structured responses
.get("/user/{id}", |Path(id): Path<u32>| async move {
    let user = User { id, name: "John" };
    Json(user)
})
```

### Multiple HTTP Methods
```rust
// Same path, different methods
.route("/resource", 
    get(get_handler)
    .post(create_handler)
    .put(update_handler)
    .delete(delete_handler)
)
```

## Compatibility Promise

- **No breaking changes** - Existing trait-based code continues to work
- **Long-term support** - Both APIs will be maintained
- **Feature parity** - New features will support both APIs where possible
- **Migration tools** - Additional tooling may be provided in future versions

## Need Help?

- 📖 Check `/examples/simple/` for comprehensive examples
- 🐛 File issues on GitHub for migration problems
- 💬 Join discussions for migration questions
- 📚 Read the updated documentation at [gotcha.rs](https://gotcha.rs)

---

**Happy migrating! 🦀✨**
