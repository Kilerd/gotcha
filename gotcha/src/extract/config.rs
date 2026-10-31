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
/// With the `openapi` feature this is every `Serialize + Schematic + Send + Sync + 'static` type,
/// and configuring a handler registers its schema as an OpenAPI component. Without `openapi`,
/// only `Serialize + Send + Sync + 'static` is required.
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
    render: Arc<dyn Fn(R) -> Response + Send + Sync>,
    /// The schema of the body this handler sends, stored type-erased with the handler.
    #[cfg(feature = "openapi")]
    schema: fn() -> crate::EnhancedSchema,
    /// Whether `schema` came from an application override. Defaults keep existing documents
    /// byte-identical, so only overridden bodies are registered as components.
    #[cfg(feature = "openapi")]
    custom: bool,
}

impl<R: 'static> ErrorHandler<R> {
    fn default_handler<E: ErrorBodyResponse>(render: impl Fn(R) -> Response + Send + Sync + 'static) -> Self {
        Self {
            render: Arc::new(render),
            #[cfg(feature = "openapi")]
            schema: schema_of::<E>(),
            #[cfg(feature = "openapi")]
            custom: false,
        }
    }

    fn custom<E: ErrorBodyResponse>(handler: impl Fn(R) -> (StatusCode, E) + Send + Sync + 'static) -> Self {
        Self {
            render: Arc::new(move |rejection| {
                let (status, body) = handler(rejection);
                json_response(status, body)
            }),
            #[cfg(feature = "openapi")]
            schema: schema_of::<E>(),
            #[cfg(feature = "openapi")]
            custom: true,
        }
    }

    fn render(&self, rejection: R) -> Response {
        (self.render)(rejection)
    }

    #[cfg(feature = "openapi")]
    fn schema(&self) -> Option<fn() -> crate::EnhancedSchema> {
        self.custom.then_some(self.schema)
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
    json_response(status, crate::ErrorBody { error: message.into() })
}

macro_rules! rejection_config {
    ($config:ident, $field:ident, $rejection:ty, $doc:literal, |$rejection_var:ident| $render:expr) => {
        #[doc = $doc]
        pub struct $config {
            handler: ErrorHandler<$rejection>,
        }

        impl $config {
            /// Replace the response used for this extractor's rejections.
            ///
            /// The handler receives the extractor's concrete rejection type and returns the
            /// status and body to send. With the `openapi` feature, the body type is registered
            /// as a component.
            pub fn error_handler<E>(self, handler: impl Fn($rejection) -> (StatusCode, E) + Send + Sync + 'static) -> Self
            where
                E: ErrorBodyResponse,
            {
                Self {
                    handler: ErrorHandler::custom(handler),
                }
            }

            pub(crate) fn render(&self, rejection: $rejection) -> Response {
                self.handler.render(rejection)
            }

            #[cfg(feature = "openapi")]
            fn schema(&self) -> Option<fn() -> crate::EnhancedSchema> {
                self.handler.schema()
            }
        }

        impl Default for $config {
            fn default() -> Self {
                Self {
                    handler: ErrorHandler::default_handler::<crate::ErrorBody>(|$rejection_var: $rejection| $render),
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
    |rejection| default_error(rejection.status(), rejection.body_text())
);

rejection_config!(
    FormConfig,
    form,
    axum::extract::rejection::FormRejection,
    "Configuration for [`Form`](crate::Form) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text())
);

rejection_config!(
    QueryConfig,
    query,
    axum::extract::rejection::QueryRejection,
    "Configuration for [`Query`](crate::Query) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text())
);

rejection_config!(
    PathConfig,
    path,
    axum::extract::rejection::PathRejection,
    "Configuration for [`Path`](crate::Path) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text())
);

rejection_config!(
    MultipartConfig,
    multipart,
    axum::extract::multipart::MultipartRejection,
    "Configuration for [`Multipart`](crate::Multipart) rejections.",
    |rejection| default_error(rejection.status(), rejection.body_text())
);

rejection_config!(
    TypedHeaderConfig,
    typed_header,
    axum_extra::typed_header::TypedHeaderRejection,
    "Configuration for [`TypedHeader`](crate::TypedHeader) rejections.",
    |rejection| default_error(StatusCode::BAD_REQUEST, rejection.to_string())
);

rejection_config!(
    HeaderConfig,
    header,
    crate::params::ParamRejection,
    "Configuration for [`Header`](crate::Header) rejections.",
    |rejection| default_error(StatusCode::BAD_REQUEST, rejection.to_string())
);

rejection_config!(
    CookieConfig,
    cookie,
    crate::params::ParamRejection,
    "Configuration for [`Cookie`](crate::Cookie) rejections.",
    |rejection| default_error(StatusCode::BAD_REQUEST, rejection.to_string())
);

/// Configuration for [`Valid`](crate::Valid) validation failures.
///
/// A malformed JSON body inside `Valid<Json<T>>` is a [`Json`](crate::Json) rejection and uses
/// [`JsonConfig`]; only a body that deserialized and then failed validation reaches this handler.
pub struct ValidConfig {
    handler: ErrorHandler<validator::ValidationErrors>,
}

impl ValidConfig {
    /// Replace the response used when validation fails.
    ///
    /// The handler receives the structured [`validator::ValidationErrors`] and returns the status
    /// and body to send. With the `openapi` feature, the body type is registered as a component.
    pub fn error_handler<E>(self, handler: impl Fn(validator::ValidationErrors) -> (StatusCode, E) + Send + Sync + 'static) -> Self
    where
        E: ErrorBodyResponse,
    {
        Self {
            handler: ErrorHandler::custom(handler),
        }
    }

    pub(crate) fn render(&self, rejection: validator::ValidationErrors) -> Response {
        self.handler.render(rejection)
    }

    #[cfg(feature = "openapi")]
    fn schema(&self) -> Option<fn() -> crate::EnhancedSchema> {
        self.handler.schema()
    }
}

impl Default for ValidConfig {
    fn default() -> Self {
        Self {
            handler: ErrorHandler::default_handler::<crate::ValidationErrorsBody>(crate::validation::validation_error_response),
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

    #[cfg(feature = "openapi")]
    pub(crate) fn schemas(&self) -> Vec<fn() -> crate::EnhancedSchema> {
        [
            self.json.schema(),
            self.form.schema(),
            self.query.schema(),
            self.path.schema(),
            self.multipart.schema(),
            self.typed_header.schema(),
            self.header.schema(),
            self.cookie.schema(),
            self.valid.schema(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// The configurations for the current request, or the defaults when the application did not
/// insert them (a plain Axum router or a direct extractor call).
pub(crate) fn configs(extensions: &Extensions) -> Arc<ExtractorConfigs> {
    static DEFAULT: Lazy<Arc<ExtractorConfigs>> = Lazy::new(|| Arc::new(ExtractorConfigs::default()));

    extensions.get::<Arc<ExtractorConfigs>>().cloned().unwrap_or_else(|| DEFAULT.clone())
}
