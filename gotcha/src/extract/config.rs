//! Per-provider rejection configuration.
//!
//! Each extractor's rejections come from a [`RejectionProvider`], which supplies the default
//! response. Applications replace a provider's handler with a [`RejectionConfig`] (for the built-in
//! providers, [`JsonConfig`] and friends) and its [`error_handler`](RejectionConfig::error_handler);
//! the handler receives that provider's concrete rejection type and returns the status and body to
//! send.
//!
//! Configs reach extractors through request extensions inserted by the framework during
//! assembly. An extractor used without that assembly (a plain Axum router, or a unit test calling
//! `from_request`) uses the defaults.

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::sync::Arc;

use axum::http::{Extensions, StatusCode};
use axum::response::Response;
use serde::Serialize;

use crate::error::json_response;
use crate::{ErrorBody, Rejected};

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

/// The source of an extractor's rejections and their default response.
///
/// Each built-in extractor has an uninhabited marker type implementing this, such as
/// [`JsonProvider`]; an application configures it with [`RejectionConfig<P>`](RejectionConfig).
/// Every item is compiled in every feature set.
pub trait RejectionProvider: 'static {
    /// The rejection the extractor produces, passed to the configured error handler.
    type Rejection: 'static;

    /// The statuses the default response can carry, each with the phrase that describes it in
    /// OpenAPI. They are documented unless the application declares its own with
    /// [`statuses`](RejectionConfig::statuses).
    const STATUSES: &'static [(StatusCode, &'static str)];

    /// The phrase for a declared status that [`STATUSES`](Self::STATUSES) does not describe.
    const FALLBACK_PHRASE: &'static str = "request rejected";

    /// The status and body sent when the application has not configured an error handler.
    fn default_error(rejection: &Self::Rejection) -> (StatusCode, ErrorBody);
}

/// A provider's rejection configuration, accepted by
/// [`Gotcha::extractor_config`](crate::Gotcha::extractor_config).
pub trait ExtractorConfig: Send + Sync + 'static {
    /// Replace this provider's configuration.
    #[doc(hidden)]
    fn apply(self, configs: &mut ExtractorConfigs);
}

struct ErrorHandler<R> {
    render: Box<dyn Fn(&R) -> Response + Send + Sync>,
    /// The schema of the body this handler sends, stored type-erased with the handler.
    #[cfg(feature = "openapi")]
    schema: fn() -> crate::EnhancedSchema,
}

#[cfg(feature = "openapi")]
fn schema_of<E: ErrorBodyResponse>() -> fn() -> crate::EnhancedSchema {
    fn build<E: ErrorBodyResponse>() -> crate::EnhancedSchema {
        E::error_schema()
    }
    build::<E>
}

/// Send `P`'s default error. Every provider's default response is rendered here.
fn render_default<P: RejectionProvider>(rejection: &P::Rejection) -> Response {
    let (status, body) = P::default_error(rejection);
    json_response(status, body)
}

/// One configured provider's rejection contract, ready for document assembly.
#[cfg(feature = "openapi")]
pub(crate) struct RejectionSpec {
    pub(crate) statuses: Vec<(StatusCode, &'static str)>,
    pub(crate) body_schema: fn() -> crate::EnhancedSchema,
}

/// The rejection configuration of provider `P`.
///
/// The default configuration changes nothing: `P`'s [`default_error`](RejectionProvider::default_error)
/// renders its rejections and its [`STATUSES`](RejectionProvider::STATUSES) are documented.
pub struct RejectionConfig<P: RejectionProvider> {
    /// The application's handler; `None` sends this provider's default error.
    handler: Option<ErrorHandler<P::Rejection>>,
    /// Statuses declared by the application's handler; `None` uses this provider's defaults.
    statuses: Option<Vec<StatusCode>>,
    provider: PhantomData<fn() -> P>,
}

impl<P: RejectionProvider> RejectionConfig<P> {
    /// Replace the response used for this extractor's rejections.
    ///
    /// The handler receives a reference to the extractor's concrete rejection type, so
    /// the extractor can return the original rejection alongside the rendered response
    /// (see [`Rejected`](crate::Rejected)), and returns the status and body to send.
    /// With the `openapi` feature, operations that document this provider's rejections
    /// reference the body type as a component.
    pub fn error_handler<E>(self, handler: impl Fn(&P::Rejection) -> (StatusCode, E) + Send + Sync + 'static) -> Self
    where
        E: ErrorBodyResponse,
    {
        Self {
            handler: Some(ErrorHandler {
                render: Box::new(move |rejection| {
                    let (status, body) = handler(rejection);
                    json_response(status, body)
                }),
                #[cfg(feature = "openapi")]
                schema: schema_of::<E>(),
            }),
            ..self
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

    /// Render `rejection` with the configured error handler, or the provider's default.
    pub fn render(&self, rejection: &P::Rejection) -> Response {
        match &self.handler {
            Some(handler) => (handler.render)(rejection),
            None => render_default::<P>(rejection),
        }
    }

    #[cfg(feature = "openapi")]
    pub(crate) fn rejection_spec(&self) -> RejectionSpec {
        RejectionSpec {
            statuses: match &self.statuses {
                Some(statuses) => statuses.iter().map(|status| (*status, describe_status::<P>(*status))).collect(),
                None => P::STATUSES.to_vec(),
            },
            body_schema: self.handler.as_ref().map_or(schema_of::<ErrorBody>(), |handler| handler.schema),
        }
    }
}

/// Describe `status` with `P`'s default phrase, or its fallback for a custom status.
#[cfg(feature = "openapi")]
fn describe_status<P: RejectionProvider>(status: StatusCode) -> &'static str {
    P::STATUSES
        .iter()
        .find(|(candidate, _)| *candidate == status)
        .map_or(P::FALLBACK_PHRASE, |(_, phrase)| *phrase)
}

impl<P: RejectionProvider> Default for RejectionConfig<P> {
    fn default() -> Self {
        Self {
            handler: None,
            statuses: None,
            provider: PhantomData,
        }
    }
}

impl<P: RejectionProvider> ExtractorConfig for RejectionConfig<P> {
    fn apply(self, configs: &mut ExtractorConfigs) {
        let id = TypeId::of::<P>();
        let config: Box<dyn ErasedConfig> = Box::new(self);
        match configs.overrides.iter_mut().find(|(each, _)| *each == id) {
            Some((_, existing)) => *existing = config,
            None => configs.overrides.push((id, config)),
        }
    }
}

/// A [`RejectionConfig`] whose provider is known only by its [`TypeId`].
trait ErasedConfig: Any + Send + Sync {
    #[cfg(feature = "openapi")]
    fn rejection_spec(&self) -> RejectionSpec;
}

impl<P: RejectionProvider> ErasedConfig for RejectionConfig<P> {
    #[cfg(feature = "openapi")]
    fn rejection_spec(&self) -> RejectionSpec {
        RejectionConfig::rejection_spec(self)
    }
}

/// The rejection configurations an application set, one per provider.
///
/// Only overrides are stored: a provider without one renders and documents its defaults.
/// Extractors read this from request extensions; when it is absent they use the defaults.
#[derive(Default)]
pub struct ExtractorConfigs {
    /// One `RejectionConfig<P>` per configured provider, keyed by `TypeId::of::<P>()`.
    overrides: Vec<(TypeId, Box<dyn ErasedConfig>)>,
}

/// The configurations of a request that carries none: every provider uses its defaults.
static NO_OVERRIDES: ExtractorConfigs = ExtractorConfigs { overrides: Vec::new() };

impl ExtractorConfigs {
    /// Replace the configuration belonging to `config`'s provider.
    pub fn set<C: ExtractorConfig>(mut self, config: C) -> Self {
        config.apply(&mut self);
        self
    }

    /// The configuration set for provider `P`, or `None` when `P` uses its defaults.
    pub fn get<P: RejectionProvider>(&self) -> Option<&RejectionConfig<P>> {
        let id = TypeId::of::<P>();
        self.overrides
            .iter()
            .find(|(each, _)| *each == id)
            .and_then(|(_, config)| (&**config as &dyn Any).downcast_ref())
    }

    /// Render `rejection` with `P`'s configured error handler, or `P`'s default.
    pub fn render<P: RejectionProvider>(&self, rejection: &P::Rejection) -> Response {
        match self.get::<P>() {
            Some(config) => config.render(rejection),
            None => render_default::<P>(rejection),
        }
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

    /// Add the rejection contract of every provider `kind` stands for to `specs`.
    ///
    /// A provider is resolved by its type: the application's override when it set one, else the
    /// provider's default statuses with an [`ErrorBody`].
    #[cfg(feature = "openapi")]
    pub(crate) fn rejection_specs(&self, kind: &crate::ExtractorKind, specs: &mut Vec<RejectionSpec>) {
        use crate::ExtractorKind;

        let key = match kind {
            ExtractorKind::Provider(key) => *key,
            ExtractorKind::All(list) => {
                for each in list.kinds() {
                    self.rejection_specs(each, specs);
                }
                return;
            }
            // The built-in extractors, and providers written before keys existed, name the built-in
            // providers by their unit variants.
            ExtractorKind::Json => provider_key::<JsonProvider>(),
            ExtractorKind::Form => provider_key::<FormProvider>(),
            ExtractorKind::Query => provider_key::<QueryProvider>(),
            ExtractorKind::Path => provider_key::<PathProvider>(),
            ExtractorKind::Multipart => provider_key::<MultipartProvider>(),
            ExtractorKind::TypedHeader => provider_key::<TypedHeaderProvider>(),
            ExtractorKind::Header => provider_key::<HeaderProvider>(),
            ExtractorKind::Cookie => provider_key::<CookieProvider>(),
            ExtractorKind::Valid => provider_key::<ValidProvider>(),
            // `ExtractorKind` is non-exhaustive: a kind added in a later release documents no
            // rejection until it is wired here.
            _ => return,
        };
        specs.push(match self.overrides.iter().find(|(id, _)| *id == key.id()) {
            Some((_, config)) => config.rejection_spec(),
            None => RejectionSpec {
                statuses: key.default_statuses().to_vec(),
                body_schema: schema_of::<ErrorBody>(),
            },
        });
    }
}

/// The documentation key of provider `P`.
#[cfg(feature = "openapi")]
const fn provider_key<P: RejectionProvider>() -> gotcha_core::ProviderKey {
    fn statuses<P: RejectionProvider>() -> &'static [(StatusCode, &'static str)] {
        P::STATUSES
    }
    gotcha_core::ProviderKey::from_parts(TypeId::of::<P>, std::any::type_name::<P>, statuses::<P>)
}

/// The [`ExtractorKind`](crate::ExtractorKind) that documents provider `P`'s rejections.
///
/// An extractor reports it from
/// [`ParameterProvider::extractor_kinds`](crate::ParameterProvider::extractor_kinds), and each
/// operation that takes the extractor documents the statuses and body of `P`'s configuration:
///
/// ```
/// use gotcha::extract::{kind, JsonProvider};
/// use gotcha::{ExtractorKind, ParameterProvider};
///
/// struct JsonLike;
///
/// impl ParameterProvider for JsonLike {
///     fn extractor_kinds() -> &'static [ExtractorKind] {
///         const { &[kind::<JsonProvider>()] }
///     }
/// }
/// ```
#[cfg(feature = "openapi")]
pub const fn kind<P: RejectionProvider>() -> crate::ExtractorKind {
    crate::ExtractorKind::Provider(provider_key::<P>())
}

/// A request's extractor configurations, held by a body extractor while another extractor
/// consumes the request.
///
/// [`take`](Self::take) removes them from the request: use it when the request goes to an extractor
/// that never reads them, such as Axum's own `Json`. Nothing can then observe that the extension is
/// gone, and a successful extraction costs one extension-map removal with no refcount bump.
///
/// [`share`](Self::share) keeps them in the request: use it when the request goes to another Gotcha
/// extractor (or any extractor that renders through these configurations) and the caller may still
/// reject afterwards. Taking them there would make the inner extractor fall back to its defaults.
/// It costs one lookup and one refcount increment per extraction.
///
/// When neither is needed, render a rejection with [`reject`] before the request is consumed.
pub struct RequestConfigs(Option<Arc<ExtractorConfigs>>);

impl RequestConfigs {
    /// Take the configurations out of a request's extensions before the request is consumed.
    pub fn take(extensions: &mut Extensions) -> Self {
        Self(extensions.remove::<Arc<ExtractorConfigs>>())
    }

    /// Keep a handle to the configurations and leave them in the request's extensions.
    pub fn share(extensions: &Extensions) -> Self {
        Self(extensions.get::<Arc<ExtractorConfigs>>().cloned())
    }

    /// Render `rejection` with `P`'s configured error handler, or `P`'s default.
    pub fn render<P: RejectionProvider>(&self, rejection: &P::Rejection) -> Response {
        self.configs().render::<P>(rejection)
    }

    /// Render `rejection` with `P`'s configuration and keep it next to the response.
    pub fn reject<P: RejectionProvider>(&self, rejection: P::Rejection) -> Rejected<P::Rejection> {
        reject_with::<P>(self.configs(), rejection)
    }

    /// The configurations in effect: the defaults when the request carried none.
    fn configs(&self) -> &ExtractorConfigs {
        self.0.as_deref().unwrap_or(&NO_OVERRIDES)
    }
}

/// Render a rejection with `P`'s configuration from the request's extensions, which are the
/// defaults when the request carries none.
///
/// For extractors that only borrow the request parts, and for body extractors that reject before
/// consuming the request. Call it from the error path only, so a successful extraction does no
/// lookup.
pub fn reject<P: RejectionProvider>(extensions: &Extensions, rejection: P::Rejection) -> Rejected<P::Rejection> {
    let configs = extensions.get::<Arc<ExtractorConfigs>>().map_or(&NO_OVERRIDES, |configs| &**configs);
    reject_with::<P>(configs, rejection)
}

fn reject_with<P: RejectionProvider>(configs: &ExtractorConfigs, rejection: P::Rejection) -> Rejected<P::Rejection> {
    let response = configs.render::<P>(&rejection);
    Rejected::new(rejection, response)
}

/// Define a built-in provider's marker type and its configuration alias.
macro_rules! builtin_provider {
    (
        $(#[$provider_doc:meta])* $provider:ident,
        $(#[$config_doc:meta])* $config:ident,
        $rejection:ty,
        |$rejection_var:ident| $default:expr,
        $fallback:literal,
        [$($status:expr => $phrase:literal),* $(,)?]
    ) => {
        $(#[$provider_doc])*
        pub enum $provider {}

        impl RejectionProvider for $provider {
            type Rejection = $rejection;
            const STATUSES: &'static [(StatusCode, &'static str)] = &[$(($status, $phrase)),*];
            const FALLBACK_PHRASE: &'static str = $fallback;

            fn default_error($rejection_var: &$rejection) -> (StatusCode, ErrorBody) {
                $default
            }
        }

        $(#[$config_doc])*
        pub type $config = RejectionConfig<$provider>;
    };
}

builtin_provider!(
    /// The [`Json`](crate::Json) rejection provider.
    JsonProvider,
    /// Configuration for [`Json`](crate::Json) rejections.
    JsonConfig,
    axum::extract::rejection::JsonRejection,
    |rejection| (rejection.status(), ErrorBody::new(rejection.body_text())),
    "invalid JSON body",
    [
        StatusCode::BAD_REQUEST => "invalid JSON body",
        StatusCode::PAYLOAD_TOO_LARGE => "JSON body too large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "missing or invalid JSON content type",
        StatusCode::UNPROCESSABLE_ENTITY => "JSON body did not match the expected type",
    ]
);

builtin_provider!(
    /// The [`Form`](crate::Form) rejection provider.
    FormProvider,
    /// Configuration for [`Form`](crate::Form) rejections.
    FormConfig,
    axum::extract::rejection::FormRejection,
    |rejection| (rejection.status(), ErrorBody::new(rejection.body_text())),
    "invalid form body",
    [
        StatusCode::BAD_REQUEST => "invalid form body",
        StatusCode::PAYLOAD_TOO_LARGE => "form body too large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "missing or invalid form content type",
        StatusCode::UNPROCESSABLE_ENTITY => "form body did not match the expected type",
    ]
);

builtin_provider!(
    /// The [`Query`](crate::Query) rejection provider.
    QueryProvider,
    /// Configuration for [`Query`](crate::Query) rejections.
    QueryConfig,
    axum::extract::rejection::QueryRejection,
    |rejection| (rejection.status(), ErrorBody::new(rejection.body_text())),
    "invalid query string",
    [StatusCode::BAD_REQUEST => "invalid query string"]
);

builtin_provider!(
    /// The [`Path`](crate::Path) rejection provider.
    PathProvider,
    /// Configuration for [`Path`](crate::Path) rejections.
    PathConfig,
    axum::extract::rejection::PathRejection,
    |rejection| (rejection.status(), ErrorBody::new(rejection.body_text())),
    "invalid path parameters",
    [StatusCode::BAD_REQUEST => "invalid path parameters"]
);

builtin_provider!(
    /// The [`Multipart`](crate::Multipart) rejection provider.
    MultipartProvider,
    /// Configuration for [`Multipart`](crate::Multipart) rejections.
    MultipartConfig,
    axum::extract::multipart::MultipartRejection,
    |rejection| (rejection.status(), ErrorBody::new(rejection.body_text())),
    "invalid multipart body",
    [StatusCode::BAD_REQUEST => "invalid multipart boundary"]
);

builtin_provider!(
    /// The [`TypedHeader`](crate::TypedHeader) rejection provider.
    TypedHeaderProvider,
    /// Configuration for [`TypedHeader`](crate::TypedHeader) rejections.
    TypedHeaderConfig,
    axum_extra::typed_header::TypedHeaderRejection,
    |rejection| (StatusCode::BAD_REQUEST, ErrorBody::new(rejection.to_string())),
    "missing or invalid typed header",
    [StatusCode::BAD_REQUEST => "missing or invalid typed header"]
);

builtin_provider!(
    /// The [`Header`](crate::Header) rejection provider.
    HeaderProvider,
    /// Configuration for [`Header`](crate::Header) rejections.
    HeaderConfig,
    crate::params::ParamRejection,
    |rejection| (StatusCode::BAD_REQUEST, ErrorBody::new(rejection.to_string())),
    "missing or invalid header parameter",
    [StatusCode::BAD_REQUEST => "missing or invalid header parameter"]
);

builtin_provider!(
    /// The [`Cookie`](crate::Cookie) rejection provider.
    CookieProvider,
    /// Configuration for [`Cookie`](crate::Cookie) rejections.
    CookieConfig,
    crate::params::ParamRejection,
    |rejection| (StatusCode::BAD_REQUEST, ErrorBody::new(rejection.to_string())),
    "missing or invalid cookie parameter",
    [StatusCode::BAD_REQUEST => "missing or invalid cookie parameter"]
);

builtin_provider!(
    /// The [`Valid`](crate::Valid) validation failure provider.
    ValidProvider,
    /// Configuration for [`Valid`](crate::Valid) validation failures.
    ///
    /// The default sends `422 Unprocessable Entity` with an [`ErrorBody`](crate::ErrorBody) whose
    /// `fields` map holds the flattened validation errors (see
    /// [`flatten_errors`](crate::validation::flatten_errors)).
    ///
    /// A malformed JSON body inside `Valid<Json<T>>` is a [`Json`](crate::Json) rejection and uses
    /// [`JsonConfig`]; only a body that deserialized and then failed validation reaches this handler.
    ///
    /// The [`error_handler`](RejectionConfig::error_handler) receives the structured
    /// [`validator::ValidationErrors`]. Its keys are the serde wire names, already remapped through
    /// [`Schematic::wire_fields`](crate::Schematic::wire_fields) by [`Valid`](crate::Valid), so a
    /// handler can report them without translating Rust identifiers. Use
    /// [`flatten_errors`](crate::validation::flatten_errors) to render the same field map the
    /// default body uses. Without [`statuses`](RejectionConfig::statuses), `422` is documented;
    /// the inner extractor's own rejections are separate.
    ValidConfig,
    validator::ValidationErrors,
    |errors| (StatusCode::UNPROCESSABLE_ENTITY, ErrorBody::with_fields("validation failed", crate::validation::flatten_errors(errors))),
    "request body failed validation",
    [StatusCode::UNPROCESSABLE_ENTITY => "request body failed validation"]
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::ParamRejection;

    fn missing() -> ParamRejection {
        ParamRejection::Missing {
            kind: "header",
            name: "x-request-id",
        }
    }

    #[test]
    fn share_leaves_the_configurations_in_the_request_and_take_removes_them() {
        let mut extensions = Extensions::new();
        let configs = ExtractorConfigs::default().set(HeaderConfig::default().error_handler(|_| (StatusCode::IM_A_TEAPOT, ErrorBody::new("teapot"))));
        extensions.insert(Arc::new(configs));

        let shared = RequestConfigs::share(&extensions);
        assert!(extensions.get::<Arc<ExtractorConfigs>>().is_some(), "an inner extractor still finds them");
        assert_eq!(shared.render::<HeaderProvider>(&missing()).status(), StatusCode::IM_A_TEAPOT);
        assert_eq!(reject::<HeaderProvider>(&extensions, missing()).response().status(), StatusCode::IM_A_TEAPOT);

        let taken = RequestConfigs::take(&mut extensions);
        assert!(extensions.get::<Arc<ExtractorConfigs>>().is_none());
        assert_eq!(taken.reject::<HeaderProvider>(missing()).response().status(), StatusCode::IM_A_TEAPOT);
        // Only `Header` is configured.
        assert_eq!(taken.render::<CookieProvider>(&missing()).status(), StatusCode::BAD_REQUEST);

        // A request without configurations uses the provider's default.
        assert_eq!(
            RequestConfigs::share(&extensions).render::<HeaderProvider>(&missing()).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            RequestConfigs::take(&mut extensions).reject::<HeaderProvider>(missing()).response().status(),
            StatusCode::BAD_REQUEST
        );
    }
}
