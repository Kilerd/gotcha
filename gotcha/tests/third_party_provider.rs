#![allow(dead_code)]

//! Extractor providers written outside Gotcha. This test crate depends on `gotcha` as an
//! extension crate would: the `msgpack` module uses only public API.

use gotcha::axum::body::{to_bytes, Body};
use gotcha::axum::http::{header, Request, StatusCode};
use gotcha::{ConfigWrapper, EmptyConfig, ExtractorConfigs, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Json, JsonConfig, Schematic};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

use msgpack::{Audited, MsgPack, MsgPackConfig, NonEmpty, NonEmptyConfig};

mod msgpack {
    use gotcha::axum::body::Bytes;
    use gotcha::axum::extract::{FromRequest, Request};
    use gotcha::axum::http::{header, StatusCode};
    use gotcha::axum::response::{IntoResponse, Response};
    use gotcha::extract::{RejectionConfig, RejectionProvider, RequestConfigs};
    use gotcha::{ErrorBody, Json, Rejected};
    use serde::de::DeserializeOwned;

    /// Why a MessagePack body was rejected.
    #[derive(Debug)]
    pub enum MsgPackRejection {
        /// The request did not declare `Content-Type: application/msgpack`.
        ContentType,
        /// The body could not be read or decoded.
        Decode(String),
    }

    impl MsgPackRejection {
        pub fn status(&self) -> StatusCode {
            match self {
                MsgPackRejection::ContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
                MsgPackRejection::Decode(_) => StatusCode::BAD_REQUEST,
            }
        }
    }

    impl std::fmt::Display for MsgPackRejection {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                MsgPackRejection::ContentType => formatter.write_str("expected `Content-Type: application/msgpack`"),
                MsgPackRejection::Decode(error) => write!(formatter, "invalid MessagePack body: {error}"),
            }
        }
    }

    /// The provider: its rejection type, the statuses to document, and the default response.
    pub enum MsgPackProvider {}

    impl RejectionProvider for MsgPackProvider {
        type Rejection = MsgPackRejection;

        const STATUSES: &'static [(StatusCode, &'static str)] = &[
            (StatusCode::BAD_REQUEST, "invalid MessagePack body"),
            (StatusCode::UNSUPPORTED_MEDIA_TYPE, "missing MessagePack content type"),
        ];
        const FALLBACK_PHRASE: &'static str = "MessagePack body rejected";

        fn default_error(rejection: &MsgPackRejection) -> (StatusCode, ErrorBody) {
            (rejection.status(), ErrorBody::new(rejection.to_string()))
        }
    }

    /// What applications pass to `extractor_config`.
    pub type MsgPackConfig = RejectionConfig<MsgPackProvider>;

    pub struct MsgPack<T>(pub T);

    impl<T, S> FromRequest<S> for MsgPack<T>
    where
        T: DeserializeOwned,
        S: Send + Sync,
    {
        type Rejection = Rejected<MsgPackRejection>;

        async fn from_request(mut req: Request, state: &S) -> Result<Self, Self::Rejection> {
            let configs = RequestConfigs::take(req.extensions_mut());
            if req.headers().get(header::CONTENT_TYPE).map(|value| value.as_bytes()) != Some(&b"application/msgpack"[..]) {
                return Err(configs.reject::<MsgPackProvider>(MsgPackRejection::ContentType));
            }
            let bytes = Bytes::from_request(req, state)
                .await
                .map_err(|rejection| configs.reject::<MsgPackProvider>(MsgPackRejection::Decode(rejection.body_text())))?;
            // A real crate decodes MessagePack here; JSON keeps the test free of a new dependency.
            serde_json::from_slice(&bytes)
                .map(MsgPack)
                .map_err(|error| configs.reject::<MsgPackProvider>(MsgPackRejection::Decode(error.to_string())))
        }
    }

    /// A wrapper that runs the extractor it wraps and forwards its rejections unchanged.
    pub struct Audited<E>(pub E);

    impl<E, S> FromRequest<S> for Audited<E>
    where
        E: FromRequest<S>,
        S: Send + Sync,
    {
        type Rejection = E::Rejection;

        async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
            E::from_request(req, state).await.map(Audited)
        }
    }

    /// The rejection of a list that decoded but holds no items.
    #[derive(Debug)]
    pub struct EmptyList;

    pub enum NonEmptyProvider {}

    impl RejectionProvider for NonEmptyProvider {
        type Rejection = EmptyList;

        const STATUSES: &'static [(StatusCode, &'static str)] = &[(StatusCode::UNPROCESSABLE_ENTITY, "empty list")];
        const FALLBACK_PHRASE: &'static str = "empty list";

        fn default_error(_: &EmptyList) -> (StatusCode, ErrorBody) {
            (StatusCode::UNPROCESSABLE_ENTITY, ErrorBody::new("the list is empty"))
        }
    }

    pub type NonEmptyConfig = RejectionConfig<NonEmptyProvider>;

    /// A wrapper that delegates to Gotcha's `Json` and rejects after it succeeds, as `Valid` does.
    pub struct NonEmpty<T>(pub Vec<T>);

    impl<T, S> FromRequest<S> for NonEmpty<T>
    where
        T: DeserializeOwned,
        S: Send + Sync,
    {
        type Rejection = Response;

        async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
            // `Json` renders its own rejections through `JsonConfig`: share the configurations
            // rather than taking them, or it would send its defaults.
            let configs = RequestConfigs::share(req.extensions());
            let Json(items) = Json::<Vec<T>>::from_request(req, state).await.map_err(IntoResponse::into_response)?;
            if items.is_empty() {
                return Err(configs.reject::<NonEmptyProvider>(EmptyList).into_response());
            }
            Ok(NonEmpty(items))
        }
    }

    #[cfg(feature = "openapi")]
    mod openapi {
        use gotcha::extract::kind;
        use gotcha::oas::{Parameter, RequestBody};
        use gotcha::{Either, ExtractorKind, Json, ParameterProvider, Schematic};

        use super::{Audited, MsgPack, MsgPackProvider, NonEmpty, NonEmptyProvider};

        impl<T: Schematic> ParameterProvider for NonEmpty<T> {
            fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
                <Json<Vec<T>> as ParameterProvider>::generate(url)
            }

            fn is_request_body() -> bool {
                true
            }

            fn extractor_kinds() -> &'static [ExtractorKind] {
                const {
                    &[
                        kind::<NonEmptyProvider>(),
                        ExtractorKind::all(<Json<Vec<T>> as ParameterProvider>::extractor_kinds),
                    ]
                }
            }
        }

        impl<T: Schematic> ParameterProvider for MsgPack<T> {
            fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
                match <gotcha::Json<T> as ParameterProvider>::generate(url) {
                    Either::Right(mut body) => {
                        let media = body.content.remove("application/json").expect("a JSON body");
                        body.content.insert("application/msgpack".to_owned(), media);
                        Either::Right(body)
                    }
                    parameters => parameters,
                }
            }

            fn is_request_body() -> bool {
                true
            }

            fn extractor_kinds() -> &'static [ExtractorKind] {
                const { &[kind::<MsgPackProvider>()] }
            }
        }

        impl<E: ParameterProvider> ParameterProvider for Audited<E> {
            fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
                E::generate(url)
            }

            fn is_request_body() -> bool {
                E::is_request_body()
            }

            fn extractor_kinds() -> &'static [ExtractorKind] {
                const { &[ExtractorKind::all(E::extractor_kinds)] }
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Schematic)]
struct Payload {
    id: u32,
}

/// The body an application chooses for MessagePack rejections.
#[derive(Debug, Serialize, Schematic)]
struct Teapot {
    reason: String,
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn upload(MsgPack(payload): MsgPack<Payload>) -> String {
    payload.id.to_string()
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn audited_upload(Audited(MsgPack(payload)): Audited<MsgPack<Payload>>) -> String {
    payload.id.to_string()
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn audited_json(Audited(Json(payload)): Audited<Json<Payload>>) -> String {
    payload.id.to_string()
}

#[cfg_attr(feature = "openapi", gotcha::api)]
async fn non_empty(NonEmpty(items): NonEmpty<u32>) -> String {
    items.len().to_string()
}

struct App {
    configured: bool,
}

impl GotchaApp for App {
    type State = ();
    type Config = EmptyConfig;

    async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
        Ok(())
    }

    #[cfg(feature = "openapi")]
    fn openapi_endpoints(&self) -> Option<gotcha::OpenApiEndpoints> {
        Some(gotcha::OpenApiEndpoints::default())
    }

    fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
        router
            .post("/upload", upload)
            .post("/audited", audited_upload)
            .post("/audited-json", audited_json)
            .post("/non-empty", non_empty)
    }

    fn extractor_configs(&self) -> ExtractorConfigs {
        if !self.configured {
            return ExtractorConfigs::default();
        }
        ExtractorConfigs::default()
            .set(
                MsgPackConfig::default()
                    .error_handler(|rejection| (StatusCode::IM_A_TEAPOT, Teapot { reason: rejection.to_string() }))
                    .statuses([StatusCode::IM_A_TEAPOT]),
            )
            .set(JsonConfig::default().error_handler(|rejection| (rejection.status(), Teapot { reason: rejection.body_text() })))
            .set(
                NonEmptyConfig::default()
                    .error_handler(|_| (StatusCode::CONFLICT, Teapot { reason: "no items".to_owned() }))
                    .statuses([StatusCode::CONFLICT]),
            )
    }
}

async fn router(configured: bool) -> gotcha::axum::Router {
    App { configured }.build_router(GotchaContext::new((), ConfigWrapper::default())).await.unwrap()
}

async fn send(router: &gotcha::axum::Router, uri: &str, content_type: Option<&str>, body: &'static str) -> (StatusCode, Option<String>, Vec<u8>) {
    let mut request = Request::post(uri);
    if let Some(content_type) = content_type {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    let response = router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).map(|value| value.to_str().unwrap().to_owned());
    (status, content_type, to_bytes(response.into_body(), 1 << 20).await.unwrap().to_vec())
}

fn json_body(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[cfg(feature = "openapi")]
async fn served_document(router: &gotcha::axum::Router) -> Value {
    let response = router
        .clone()
        .oneshot(Request::get("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    json_body(&to_bytes(response.into_body(), 1 << 20).await.unwrap())
}

#[cfg(feature = "openapi")]
fn responses<'a>(spec: &'a Value, path: &str) -> &'a serde_json::Map<String, Value> {
    spec["paths"][path]["post"]["responses"].as_object().expect("operation has responses")
}

#[cfg(feature = "openapi")]
fn statuses<'a>(spec: &'a Value, path: &str) -> Vec<&'a str> {
    responses(spec, path).keys().map(String::as_str).collect()
}

#[tokio::test]
async fn an_unconfigured_provider_sends_and_documents_its_defaults() {
    let router = router(false).await;

    let (status, content_type, body) = send(&router, "/upload", None, "{}").await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert_eq!(json_body(&body), json!({"error": "expected `Content-Type: application/msgpack`"}));

    let (status, _, body) = send(&router, "/upload", Some("application/msgpack"), "{").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(&body),
        json!({"error": "invalid MessagePack body: EOF while parsing an object at line 1 column 1"})
    );

    let (status, _, body) = send(&router, "/upload", Some("application/msgpack"), r#"{"id":7}"#).await;
    assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"7"[..]));

    #[cfg(feature = "openapi")]
    {
        let spec = served_document(&router).await;
        assert_eq!(statuses(&spec, "/upload"), ["200", "400", "415"]);
        for (status, description) in [
            ("400", "Request rejected: invalid MessagePack body"),
            ("415", "Request rejected: missing MessagePack content type"),
        ] {
            let response = &responses(&spec, "/upload")[status];
            assert_eq!(response["description"], description);
            assert_eq!(response["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/ErrorBody");
        }
        let request_body = &spec["paths"]["/upload"]["post"]["requestBody"]["content"];
        assert_eq!(request_body["application/msgpack"]["schema"]["$ref"], "#/components/schemas/Payload");
        assert!(spec["components"]["schemas"].get("Teapot").is_none());
    }
}

#[tokio::test]
async fn a_configured_provider_sends_and_documents_the_configured_handler() {
    let router = router(true).await;

    for (content_type, body, reason) in [
        (None, "{}", "expected `Content-Type: application/msgpack`"),
        (
            Some("application/msgpack"),
            "{",
            "invalid MessagePack body: EOF while parsing an object at line 1 column 1",
        ),
    ] {
        let (status, response_type, response) = send(&router, "/upload", content_type, body).await;
        assert_eq!(status, StatusCode::IM_A_TEAPOT);
        assert_eq!(response_type.as_deref(), Some("application/json"));
        assert_eq!(json_body(&response), json!({ "reason": reason }));
    }

    #[cfg(feature = "openapi")]
    {
        let spec = served_document(&router).await;
        assert_eq!(statuses(&spec, "/upload"), ["200", "418"]);
        let teapot = &responses(&spec, "/upload")["418"];
        assert_eq!(teapot["description"], "Request rejected: MessagePack body rejected");
        assert_eq!(teapot["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/Teapot");
        assert!(spec["components"]["schemas"].get("Teapot").is_some());
    }
}

#[tokio::test]
async fn a_wrapper_sends_and_documents_the_rejections_of_the_extractor_it_wraps() {
    let router = router(true).await;

    let (status, _, body) = send(&router, "/audited", None, "{}").await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
    assert_eq!(json_body(&body), json!({"reason": "expected `Content-Type: application/msgpack`"}));
    // The application's `JsonConfig` reaches the wrapped `Json`.
    let (status, _, body) = send(&router, "/audited-json", None, "{}").await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(json_body(&body), json!({"reason": "Expected request with `Content-Type: application/json`"}));

    #[cfg(feature = "openapi")]
    {
        let spec = served_document(&router).await;
        assert_eq!(responses(&spec, "/audited"), responses(&spec, "/upload"));
        assert_eq!(statuses(&spec, "/audited-json"), ["200", "400", "413", "415", "422"]);
        let unsupported = &responses(&spec, "/audited-json")["415"];
        assert_eq!(unsupported["description"], "Request rejected: missing or invalid JSON content type");
        assert_eq!(unsupported["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/Teapot");
    }
}

#[tokio::test]
async fn a_wrapper_that_delegates_to_a_gotcha_extractor_keeps_both_configurations() {
    let configured = router(true).await;

    // The inner `Json` still finds the configurations the wrapper shared.
    let (status, _, body) = send(&configured, "/non-empty", None, "[1]").await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(json_body(&body), json!({"reason": "Expected request with `Content-Type: application/json`"}));
    // The wrapper rejects after `Json` succeeded, with its own configuration.
    let (status, _, body) = send(&configured, "/non-empty", Some("application/json"), "[]").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json_body(&body), json!({"reason": "no items"}));
    let (status, _, body) = send(&configured, "/non-empty", Some("application/json"), "[1, 2]").await;
    assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"2"[..]));

    let defaults = router(false).await;
    let (status, _, body) = send(&defaults, "/non-empty", Some("application/json"), "[]").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json_body(&body), json!({"error": "the list is empty"}));

    #[cfg(feature = "openapi")]
    {
        let spec = served_document(&configured).await;
        assert_eq!(statuses(&spec, "/non-empty"), ["200", "400", "409", "413", "415", "422"]);
        let conflict = &responses(&spec, "/non-empty")["409"];
        assert_eq!(conflict["description"], "Request rejected: empty list");
        assert_eq!(conflict["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/Teapot");

        let spec = served_document(&defaults).await;
        assert_eq!(statuses(&spec, "/non-empty"), ["200", "400", "413", "415", "422"]);
        let unprocessable = &responses(&spec, "/non-empty")["422"];
        assert_eq!(
            unprocessable["description"],
            "Request rejected: empty list / JSON body did not match the expected type"
        );
        assert_eq!(unprocessable["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/ErrorBody");
    }
}
