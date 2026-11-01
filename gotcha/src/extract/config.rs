//! Per-extractor rejection configuration.
//!
//! Each extractor has a config type with a default error handler. Applications replace a
//! provider's handler with [`error_handler`](JsonConfig::error_handler); the handler receives
//! that provider's concrete rejection type and returns the status and body to send.
//!
//! Configs reach extractors through request extensions inserted by the framework during
//! assembly. An extractor used without that assembly (a plain Axum router, or a unit test calling
//! `from_request`) uses the defaults.

use std::sync::Arc;

use axum::http::{Extensions, StatusCode};
use axum::response::Response;
use once_cell::sync::Lazy;
use serde::Serialize;

use crate::error::json_response;

/// A body type accepted by an extractor error handler.
///
/// With the `openapi` feature this is every `Serialize + Schematic + Send + Sync + 'static` type;
/// an operation that documents this provider's rejections references its schema as an OpenAPI
/// component. Without `openapi`, only `Serialize + Send + Sync + 'static` is required.
#[cfg(feature = "openapi")]
pub trait ErrorBodyResponse: Serialize + crate::Schematic + Send + Sync + 'static {
    /// The schema registered for this body type.
    #[doc(hidden)]
    fn error_schema() -> crate::EnhancedSchema {
        <Self as crate::Schematic>::generate_schema()
    }
}

#[cfg(feature = "openapi")]
impl<T: Serialize + crate::Schematic + Send + Sync + 'static> ErrorBodyResponse for T {}

/// A body type accepted by an extractor error handler.
#[cfg(not(feature = "openapi"))]
pub trait ErrorBodyResponse: Serialize + Send + Sync + 'static {}

#[cfg(not(feature = "openapi"))]
impl<T: Serialize + Send + Sync + 'static> ErrorBodyResponse for T {}

/// A provider's rejection configuration, accepted by
/// [`Gotcha::extractor_config`](crate::Gotcha::extractor_config).
pub trait ExtractorConfig: Send + Sync + 'static {
    /// Replace this provider's configuration.
    #[doc(hidden)]
    fn apply(self, configs: &mut ExtractorConfigs);
}

struct ErrorHandler<R> {
    render: Arc<dyn Fn(&R) -> Response + Send + Sync>,
    /// The schema of the body this handler sends, stored type-erased with the handler.
    #[cfg(feature = "openapi")]
    schema: fn() -> crate::EnhancedSchema,
}

impl<R: 'static> ErrorHandler<R> {
    fn default_handler<E: ErrorBodyResponse>(render: impl Fn(&R) -> Response + Send + Sync + 'static) -> Self {
        Self {
            render: Arc::new(render),
            #[cfg(feature = "openapi")]
            schema: schema_of::<E>(),
        }
    }

    fn custom<E: ErrorBodyResponse>(handler: impl Fn(&R) -> (StatusCode, E) + Send + Sync + 'static) -> Self {
        Self {
            render: Arc::new(move |rejection| {
                let (status, body) = handler(rejection);
                json_response(status, body)
            }),
            #[cfg(feature = "openapi")]
            schema: schema_of::<E>(),
        }
    }

    fn render(&self, rejection: &R) -> Response {
        (self.render)(rejection)
    }

    /// The schema of the body this handler always sends, including the default `ErrorBody`.
    #[cfg(feature = "openapi")]
    fn body_schema(&self) -> fn() -> crate::EnhancedSchema {
        self.schema
    }
}

#[cfg(feature = "openapi")]
fn schema_of<E: ErrorBodyResponse>() -> fn() -> crate::EnhancedSchema {
    fn build<E: ErrorBodyResponse>() -> crate::EnhancedSchema {
        E::error_schema()
    }
    build::<E>
}

fn default_error(status: StatusCode, message: impl Into<String>) -> Response {
    json_response(status, crate::ErrorBody::new(message))
}

/// A default rejection status and the phrase used to describe it in OpenAPI.
#[cfg(feature = "openapi")]
pub(crate) type StatusDescription = (StatusCode, &'static str);

/// One configured provider's rejection contract, ready for document assembly.
#[cfg(feature = "openapi")]
pub(crate) struct RejectionSpec {
    pub(crate) statuses: Vec<StatusDescription>,
    pub(crate) body_schema: fn() -> crate::EnhancedSchema,
}

/// Describe `status` with its default phrase, or the provider's fallback for a custom status.
#[cfg(feature = "openapi")]
fn describe_status(defaults: &'static [StatusDescription], status: StatusCode, fallback: &'static str) -> &'static str {
    defaults
        .iter()
        .find(|(candidate, _)| *candidate == status)
        .map(|(_, phrase)| *phrase)
        .unwrap_or(fallback)
}

macro_rules! rejection_config {
    ($config:ident, $field:ident, $rejection:ty, $doc:literal, |$rejection_var:ident| $render:expr, $fallback:literal, [$($status:expr => $phrase:literal),* $(,)?]) => {
        #[doc = $doc]
        pub struct $config {
            handler: ErrorHandler<$rejection>,
            /// Statuses declared by the application's handler; `None` uses this provider's defaults.
            statuses: Option<Vec<StatusCode>>,
        }

        impl $config {
            /// The statuses this provider rejects with when it uses its default handler.
            #[cfg(feature = "openapi")]
            const DEFAULT_STATUSES: &'static [StatusDescription] = &[$(($status, $phrase)),*];

            /// Replace the response used for this extractor's rejections.
            ///
            /// The handler receives a reference to the extractor's concrete rejection type, so
            /// the extractor can return the original rejection alongside the rendered response
            /// (see [`Rejected`](crate::Rejected)), and returns the status and body to send.
            /// With the `openapi` feature, operations that document this provider's rejections
            /// reference the body type as a component.
            pub fn error_handler<E>(self, handler: impl Fn(&$rejection) -> (StatusCode, E) + Send + Sync + 'static) -> Self
            where
                E: ErrorBodyResponse,
            {
                Self {
                    handler: ErrorHandler::custom(handler),
                    statuses: self.statuses,
                }
            }

            /// Declare the statuses the configured error handler can return.
            ///
            /// Document generation adds one rejection response per status, with the body type
            /// passed to [`error_handler`](Self::error_handler). Without this call, the
            /// provider's default statuses are documented. This only affects documentation.
            pub fn statuses<I>(mut self, statuses: I) -> Self
            where
                I: IntoIterator<Item = StatusCode>,
            {
                self.statuses = Some(statuses.into_iter().collect());
                self
            }

            pub(crate) fn render(&self, rejection: &$rejection) -> Response {
                self.handler.render(rejection)
            }

            #[cfg(feature = "openapi")]
            pub(crate) fn rejection_spec(&self) -> RejectionSpec {
                RejectionSpec {
                    statuses: match &self.statuses {
                        Some(statuses) => statuses
                            .iter()
                            .map(|status| (*status, describe_status(Self::DEFAULT_STATUSES, *status, $fallback)))
                            .collect(),
                        None => Self::DEFAULT_STATUSES.to_vec(),
                    },
                    body_schema: self.handler.body_schema(),
                }
            }
        }

        impl Default for $config {
            fn default() -> Self {
                Self {
                    handler: ErrorHandler::default_handler::<crate::ErrorBody>(|$rejection_var: &$rejection| $render),
                    statuses: None,
                }
            }
        }

        impl ExtractorConfig for $config {
            fn apply(self, configs: &mut ExtractorConfigs) {
                configs.$field = self;
            }
        }
    };
}

rejection_config!(
    JsonConfig,
    json,
    axum::extract::rejection::JsonRejection,
    "Configuration for [`Json`](crate::Json) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text()),
    "invalid JSON body",
    [
        StatusCode::BAD_REQUEST => "invalid JSON body",
        StatusCode::PAYLOAD_TOO_LARGE => "JSON body too large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "missing or invalid JSON content type",
        StatusCode::UNPROCESSABLE_ENTITY => "JSON body did not match the expected type",
    ]
);

rejection_config!(
    FormConfig,
    form,
    axum::extract::rejection::FormRejection,
    "Configuration for [`Form`](crate::Form) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text()),
    "invalid form body",
    [
        StatusCode::BAD_REQUEST => "invalid form body",
        StatusCode::PAYLOAD_TOO_LARGE => "form body too large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "missing or invalid form content type",
        StatusCode::UNPROCESSABLE_ENTITY => "form body did not match the expected type",
    ]
);

rejection_config!(
    QueryConfig,
    query,
    axum::extract::rejection::QueryRejection,
    "Configuration for [`Query`](crate::Query) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text()),
    "invalid query string",
    [StatusCode::BAD_REQUEST => "invalid query string"]
);

rejection_config!(
    PathConfig,
    path,
    axum::extract::rejection::PathRejection,
    "Configuration for [`Path`](crate::Path) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text()),
    "invalid path parameters",
    [StatusCode::BAD_REQUEST => "invalid path parameters"]
);

rejection_config!(
    MultipartConfig,
    multipart,
    axum::extract::multipart::MultipartRejection,
    "Configuration for [`Multipart`](crate::Multipart) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text()),
    "invalid multipart body",
    [StatusCode::BAD_REQUEST => "invalid multipart boundary"]
);

rejection_config!(
    TypedHeaderConfig,
    typed_header,
    axum_extra::typed_header::TypedHeaderRejection,
    "Configuration for [`TypedHeader`](crate::TypedHeader) rejections.",
    |rejection| default_error(StatusCode::BAD_REQUEST, rejection.to_string()),
    "missing or invalid typed header",
    [StatusCode::BAD_REQUEST => "missing or invalid typed header"]
);

rejection_config!(
    HeaderConfig,
    header,
    crate::params::ParamRejection,
    "Configuration for [`Header`](crate::Header) rejections.",
    |rejection| default_error(StatusCode::BAD_REQUEST, rejection.to_string()),
    "missing or invalid header parameter",
    [StatusCode::BAD_REQUEST => "missing or invalid header parameter"]
);

rejection_config!(
    CookieConfig,
    cookie,
    crate::params::ParamRejection,
    "Configuration for [`Cookie`](crate::Cookie) rejections.",
    |rejection| default_error(StatusCode::BAD_REQUEST, rejection.to_string()),
    "missing or invalid cookie parameter",
    [StatusCode::BAD_REQUEST => "missing or invalid cookie parameter"]
);

/// Configuration for [`Valid`](crate::Valid) validation failures.
///
/// The default sends `422 Unprocessable Entity` with an [`ErrorBody`](crate::ErrorBody) whose
/// `fields` map holds the flattened validation errors (see
/// [`flatten_errors`](crate::validation::flatten_errors)).
///
/// A malformed JSON body inside `Valid<Json<T>>` is a [`Json`](crate::Json) rejection and uses
/// [`JsonConfig`]; only a body that deserialized and then failed validation reaches this handler.
/// The errors are already keyed by serde wire names, so a handler can report them without
/// translating Rust identifiers.
pub struct ValidConfig {
    handler: ErrorHandler<validator::ValidationErrors>,
    /// Statuses declared by the application's handler; `None` uses this provider's defaults.
    statuses: Option<Vec<StatusCode>>,
}

impl ValidConfig {
    /// The statuses validation rejects with when it uses its default handler.
    #[cfg(feature = "openapi")]
    const DEFAULT_STATUSES: &'static [StatusDescription] = &[(StatusCode::UNPROCESSABLE_ENTITY, "request body failed validation")];

    /// Replace the response used when validation fails.
    ///
    /// The handler receives a reference to the structured [`validator::ValidationErrors`] and
    /// returns the status and body to send. Its keys are the serde wire names, already remapped
    /// through [`Schematic::wire_fields`](crate::Schematic::wire_fields) by [`Valid`](crate::Valid).
    /// Use [`flatten_errors`](crate::validation::flatten_errors) to render the same field map the
    /// default body uses. With the `openapi` feature, operations that document validation failures
    /// reference the body type as a component.
    pub fn error_handler<E>(self, handler: impl Fn(&validator::ValidationErrors) -> (StatusCode, E) + Send + Sync + 'static) -> Self
    where
        E: ErrorBodyResponse,
    {
        Self {
            handler: ErrorHandler::custom(handler),
            statuses: self.statuses,
        }
    }

    /// Declare the statuses the configured error handler can return.
    ///
    /// Document generation adds one rejection response per status, with the body type passed to
    /// [`error_handler`](Self::error_handler). Without this call, `422` is documented. This only
    /// affects documentation; the inner extractor's own rejections are separate.
    pub fn statuses<I>(mut self, statuses: I) -> Self
    where
        I: IntoIterator<Item = StatusCode>,
    {
        self.statuses = Some(statuses.into_iter().collect());
        self
    }

    pub(crate) fn render(&self, rejection: &validator::ValidationErrors) -> Response {
        self.handler.render(rejection)
    }

    #[cfg(feature = "openapi")]
    pub(crate) fn rejection_spec(&self) -> RejectionSpec {
        RejectionSpec {
            statuses: match &self.statuses {
                Some(statuses) => statuses
                    .iter()
                    .map(|status| (*status, describe_status(Self::DEFAULT_STATUSES, *status, "request body failed validation")))
                    .collect(),
                None => Self::DEFAULT_STATUSES.to_vec(),
            },
            body_schema: self.handler.body_schema(),
        }
    }
}

impl Default for ValidConfig {
    fn default() -> Self {
        Self {
            handler: ErrorHandler::default_handler::<crate::ErrorBody>(crate::validation::validation_error_response),
            statuses: None,
        }
    }
}

impl ExtractorConfig for ValidConfig {
    fn apply(self, configs: &mut ExtractorConfigs) {
        configs.valid = self;
    }
}

/// The rejection configurations in effect for an application, one per extractor.
///
/// The defaults are the responses Gotcha uses when an application has not overridden them.
/// Extractors read this from request extensions; when it is absent they use
/// [`ExtractorConfigs::default`].
#[derive(Default)]
pub struct ExtractorConfigs {
    pub(crate) json: JsonConfig,
    pub(crate) form: FormConfig,
    pub(crate) query: QueryConfig,
    pub(crate) path: PathConfig,
    pub(crate) multipart: MultipartConfig,
    pub(crate) typed_header: TypedHeaderConfig,
    pub(crate) header: HeaderConfig,
    pub(crate) cookie: CookieConfig,
    pub(crate) valid: ValidConfig,
}

impl ExtractorConfigs {
    /// Replace the configuration belonging to `config`'s provider.
    pub fn set<C: ExtractorConfig>(mut self, config: C) -> Self {
        config.apply(&mut self);
        self
    }

    /// Insert these configurations into every request's extensions.
    ///
    /// The framework's default assembly applies this as the outermost layer, after business
    /// routes, documentation, `finish_router`, and `app_layer` middleware. An application that
    /// overrides [`GotchaApp::build_router`](crate::GotchaApp::build_router) itself must call
    /// this last; otherwise its extractors fall back to the defaults.
    pub fn layer(self, router: axum::Router) -> axum::Router {
        router.layer(axum::Extension(Arc::new(self)))
    }

    /// The statuses and body schema to document for `kind`'s rejections.
    #[cfg(feature = "openapi")]
    pub(crate) fn rejection_spec(&self, kind: crate::ExtractorKind) -> RejectionSpec {
        match kind {
            crate::ExtractorKind::Json => self.json.rejection_spec(),
            crate::ExtractorKind::Form => self.form.rejection_spec(),
            crate::ExtractorKind::Query => self.query.rejection_spec(),
            crate::ExtractorKind::Path => self.path.rejection_spec(),
            crate::ExtractorKind::Multipart => self.multipart.rejection_spec(),
            crate::ExtractorKind::TypedHeader => self.typed_header.rejection_spec(),
            crate::ExtractorKind::Header => self.header.rejection_spec(),
            crate::ExtractorKind::Cookie => self.cookie.rejection_spec(),
            crate::ExtractorKind::Valid => self.valid.rejection_spec(),
            // `ExtractorKind` is non-exhaustive: a provider added in a later release documents no
            // rejection until it is wired here. An empty status list adds no response.
            _ => RejectionSpec {
                statuses: Vec::new(),
                body_schema: schema_of::<crate::ErrorBody>(),
            },
        }
    }
}

/// The configurations for the current request, or the defaults when the application did not
/// insert them (a plain Axum router or a direct extractor call).
///
/// Parts-based extractors call this only from their error path, so a successful extraction does
/// not touch the shared [`Arc`]'s refcount.
pub(crate) fn configs(extensions: &Extensions) -> Arc<ExtractorConfigs> {
    extensions.get::<Arc<ExtractorConfigs>>().cloned().unwrap_or_else(|| Arc::clone(defaults()))
}

/// The configurations taken out of a request that is about to be consumed, or the defaults.
///
/// A body extractor delegates to Axum's extractor, which consumes the request and its extensions.
/// Removing the extension first keeps the [`Arc`] alive without a refcount bump on the success
/// path; only the delegated extractor sees the request, and it is dropped there, so nothing can
/// observe that the extension is gone. The cost is one extension-map removal per body extraction.
pub(crate) struct TakenConfigs(Option<Arc<ExtractorConfigs>>);

impl TakenConfigs {
    pub(crate) fn take(extensions: &mut Extensions) -> Self {
        Self(extensions.remove::<Arc<ExtractorConfigs>>())
    }
}

impl std::ops::Deref for TakenConfigs {
    type Target = ExtractorConfigs;

    fn deref(&self) -> &ExtractorConfigs {
        self.0.as_deref().unwrap_or_else(|| &**defaults())
    }
}

/// The shared default configuration, used when the application did not insert its own.
fn defaults() -> &'static Arc<ExtractorConfigs> {
    static DEFAULT: Lazy<Arc<ExtractorConfigs>> = Lazy::new(|| Arc::new(ExtractorConfigs::default()));

    &DEFAULT
}
