#![cfg(feature = "openapi")]
#![allow(dead_code)]

//! Operations document the rejection responses of Gotcha's extractor providers by default.

use std::collections::HashMap;

use gotcha::axum::body::{to_bytes, Body as AxumBody};
use gotcha::axum::extract::{Form as AxumForm, Json as AxumJson, Path as AxumPath, Query as AxumQuery};
use gotcha::axum::http::{HeaderMap, Method, Request, StatusCode};
use gotcha::{
    api, ConfigWrapper, Cookie, CookieParam, EmptyConfig, Form, Gotcha, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, Header, HeaderParam, Json,
    JsonConfig, Multipart, OpenApiEndpoints, Operable, ParameterProvider, Path, Query, QueryConfig, Schematic, State, TypedHeader, Valid,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Clone, Debug, Deserialize, Serialize, Schematic)]
struct Payload {
    id: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Schematic)]
struct Filter {
    page: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Schematic)]
struct DeclaredBody {
    message: String,
}

#[derive(Clone, Debug, Deserialize, Schematic, gotcha::Validate)]
#[validate(crate = "gotcha::validator")]
struct Checked {
    #[validate(range(min = 1))]
    id: u32,
}

/// A body the application chooses for a provider's rejections.
#[derive(Clone, Debug, Serialize, Schematic)]
struct Problem {
    code: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Schematic)]
struct RequestId(String);

impl HeaderParam for RequestId {
    const NAME: &'static str = "x-request-id";
    fn parse(raw: &str) -> Result<Self, String> {
        Ok(RequestId(raw.to_owned()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Schematic)]
struct Session(String);

impl CookieParam for Session {
    const NAME: &'static str = "session";
    fn parse(raw: &str) -> Result<Self, String> {
        Ok(Session(raw.to_owned()))
    }
}

// One handler per provider, so each operation documents exactly that provider's statuses.

#[api]
async fn json_body(_body: Json<Payload>) -> String {
    String::new()
}

#[api]
async fn form_body(_body: Form<Payload>) -> String {
    String::new()
}

#[api]
async fn query_params(_query: Query<Filter>) -> String {
    String::new()
}

#[api]
async fn path_param(Path(_id): Path<u32>) -> String {
    String::new()
}

#[api]
async fn multipart_body(_body: Multipart) -> String {
    String::new()
}

#[api]
async fn typed_header(TypedHeader(_agent): TypedHeader<gotcha::headers::UserAgent>) -> String {
    String::new()
}

#[api]
async fn header_param(Header(_id): Header<RequestId>) -> String {
    String::new()
}

#[api]
async fn cookie_param(Cookie(_session): Cookie<Session>) -> String {
    String::new()
}

#[api]
async fn valid_json(Valid(Json(_body)): Valid<Json<Checked>>) -> String {
    String::new()
}

#[api]
async fn valid_axum_json(Valid(AxumJson(_body)): Valid<AxumJson<Checked>>) -> String {
    String::new()
}

/// Axum's originals keep their plain-text rejections and document none of them.
#[api]
async fn raw_axum(
    State(_): State<()>, _headers: HeaderMap, AxumPath(id): AxumPath<u32>, AxumQuery(_query): AxumQuery<Filter>, AxumJson(_body): AxumJson<Payload>,
) -> AxumJson<Payload> {
    AxumJson(Payload { id })
}

#[api]
async fn raw_axum_form(_body: AxumForm<Payload>) -> String {
    String::new()
}

/// `Option` relaxes the body's requiredness, not its rejection contract: a present but malformed
/// body is still rejected.
#[api]
async fn optional_json(_body: Option<Json<Payload>>) -> String {
    String::new()
}

/// Two providers that both reject with `400`.
#[api]
async fn json_and_query(_query: Query<Filter>, _body: Json<Payload>) -> String {
    String::new()
}

#[api(errors(response(status = 400, body = "Problem", description = "Explicit rejection")))]
async fn explicit_error(_body: Json<Payload>) -> Result<String, gotcha::HttpError> {
    Ok(String::new())
}

#[api(responses(response(status = 422, body = "Problem", description = "Explicit unprocessable")), drop_default)]
async fn explicit_response(_body: Json<Payload>) -> String {
    String::new()
}

/// `#[api(skip)]` removes the argument from the document entirely, including its rejections.
#[api]
async fn skipped_json(#[api(skip)] _body: Json<Payload>) -> String {
    String::new()
}

/// The only provider rejection status is overridden explicitly, so no default body is referenced.
#[api(errors(response(status = 400, body = "Problem")))]
async fn query_explicit(_query: Query<Filter>) -> Result<String, gotcha::HttpError> {
    Ok(String::new())
}

#[api(request_body(body = "DeclaredBody"))]
async fn declared_body(Path(_id): Path<u32>, _query: Query<Filter>, _body: Json<Payload>) -> String {
    String::new()
}

#[api(skip_response, responses(response(status = 200, body = "String", content_type = "text/plain")))]
async fn skipped(_query: Query<Filter>, _body: Json<Payload>) -> Json<Payload> {
    Json(Payload { id: 1 })
}

#[api]
async fn plain() -> String {
    String::new()
}

fn operable<H, T>(_: H) -> &'static Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().expect("handler is registered")
}

fn document<const N: usize>(routes: [(&str, Method, &'static Operable); N]) -> Value {
    let routes: HashMap<_, _> = routes.into_iter().map(|(path, method, op)| ((path.to_owned(), method), op)).collect();
    serde_json::to_value(gotcha::openapi::generate_openapi(routes)).unwrap()
}

fn one(op: &'static Operable, path: &str) -> Value {
    document([(path, Method::POST, op)])
}

fn responses<'a>(spec: &'a Value, path: &str) -> &'a serde_json::Map<String, Value> {
    spec["paths"][path]["post"]["responses"].as_object().expect("operation has responses")
}

fn statuses<'a>(spec: &'a Value, path: &str) -> Vec<&'a str> {
    responses(spec, path).keys().map(String::as_str).collect()
}

fn error_body_ref(response: &Value) -> &str {
    response["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .expect("JSON response schema")
}

#[test]
fn each_provider_documents_the_statuses_its_rejections_can_return() {
    let json_doc = one(operable(json_body), "/json");
    assert_eq!(statuses(&json_doc, "/json"), ["200", "400", "413", "415", "422"]);
    for (status, description) in [
        ("400", "Request rejected: invalid JSON body"),
        ("413", "Request rejected: JSON body too large"),
        ("415", "Request rejected: missing or invalid JSON content type"),
        ("422", "Request rejected: JSON body did not match the expected type"),
    ] {
        assert_eq!(responses(&json_doc, "/json")[status]["description"], description);
        assert_eq!(error_body_ref(&responses(&json_doc, "/json")[status]), "#/components/schemas/ErrorBody");
    }
    assert!(json_doc["components"]["schemas"].get("ErrorBody").is_some());

    let form_doc = one(operable(form_body), "/form");
    assert_eq!(statuses(&form_doc, "/form"), ["200", "400", "413", "415", "422"]);
    assert_eq!(responses(&form_doc, "/form")["400"]["description"], "Request rejected: invalid form body");

    let query_doc = one(operable(query_params), "/query");
    assert_eq!(statuses(&query_doc, "/query"), ["200", "400"]);
    assert_eq!(responses(&query_doc, "/query")["400"]["description"], "Request rejected: invalid query string");

    let path_doc = one(operable(path_param), "/path/{id}");
    assert_eq!(statuses(&path_doc, "/path/{id}"), ["200", "400"]);
    assert_eq!(
        responses(&path_doc, "/path/{id}")["400"]["description"],
        "Request rejected: invalid path parameters"
    );

    let multipart_doc = one(operable(multipart_body), "/multipart");
    assert_eq!(statuses(&multipart_doc, "/multipart"), ["200", "400"]);
    assert_eq!(
        responses(&multipart_doc, "/multipart")["400"]["description"],
        "Request rejected: invalid multipart boundary"
    );

    let typed_doc = one(operable(typed_header), "/typed");
    assert_eq!(statuses(&typed_doc, "/typed"), ["200", "400"]);
    assert_eq!(
        responses(&typed_doc, "/typed")["400"]["description"],
        "Request rejected: missing or invalid typed header"
    );

    let header_doc = one(operable(header_param), "/header");
    assert_eq!(statuses(&header_doc, "/header"), ["200", "400"]);
    assert_eq!(
        responses(&header_doc, "/header")["400"]["description"],
        "Request rejected: missing or invalid header parameter"
    );

    let cookie_doc = one(operable(cookie_param), "/cookie");
    assert_eq!(statuses(&cookie_doc, "/cookie"), ["200", "400"]);
    assert_eq!(
        responses(&cookie_doc, "/cookie")["400"]["description"],
        "Request rejected: missing or invalid cookie parameter"
    );

    let optional_doc = one(operable(optional_json), "/optional");
    assert_eq!(statuses(&optional_doc, "/optional"), ["200", "400", "413", "415", "422"]);
}

#[test]
fn valid_documents_validation_plus_its_inner_extractor() {
    let gotcha = one(operable(valid_json), "/valid");
    assert_eq!(statuses(&gotcha, "/valid"), ["200", "400", "413", "415", "422"]);
    // Same status, same configured body: one response whose description names both causes.
    assert_eq!(
        responses(&gotcha, "/valid")["422"]["description"],
        "Request rejected: request body failed validation / JSON body did not match the expected type"
    );
    assert_eq!(error_body_ref(&responses(&gotcha, "/valid")["422"]), "#/components/schemas/ErrorBody");

    // Axum's original Json contributes no JSON rejection statuses; only validation remains.
    let axum = one(operable(valid_axum_json), "/valid-axum");
    assert_eq!(statuses(&axum, "/valid-axum"), ["200", "422"]);
    assert_eq!(
        responses(&axum, "/valid-axum")["422"]["description"],
        "Request rejected: request body failed validation"
    );
}

#[test]
fn a_custom_handler_documents_its_body_and_declared_statuses() {
    let app = Gotcha::new()
        .extractor_config(
            JsonConfig::default()
                .error_handler(|_| (StatusCode::BAD_REQUEST, Problem { code: 1 }))
                .statuses([StatusCode::BAD_REQUEST, StatusCode::CONFLICT]),
        )
        .post("/json", json_body);
    let spec = serde_json::to_value(app.into_openapi()).unwrap();
    assert_eq!(statuses(&spec, "/json"), ["200", "400", "409"]);
    for status in ["400", "409"] {
        assert_eq!(
            responses(&spec, "/json")[status]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/Problem"
        );
    }
    // A custom status keeps the provider's phrase.
    assert_eq!(responses(&spec, "/json")["409"]["description"], "Request rejected: invalid JSON body");
    assert!(spec["components"]["schemas"].get("Problem").is_some());
    assert!(spec["components"]["schemas"].get("ErrorBody").is_none(), "the default body is no longer used");
}

#[test]
fn a_configured_body_is_registered_only_when_an_operation_references_it() {
    // The override belongs to `Json`, but the only route takes `Query`. Its rejections document
    // the default `ErrorBody`; the unused `Problem` must not appear as an orphan component.
    let app = Gotcha::new()
        .extractor_config(JsonConfig::default().error_handler(|_| (StatusCode::BAD_REQUEST, Problem { code: 1 })))
        .post("/query", query_params);
    let spec = serde_json::to_value(app.into_openapi()).unwrap();
    assert_eq!(statuses(&spec, "/query"), ["200", "400"]);
    assert_eq!(error_body_ref(&responses(&spec, "/query")["400"]), "#/components/schemas/ErrorBody");
    assert!(
        spec["components"]["schemas"].get("Problem").is_none(),
        "an unreferenced configured body is not a component"
    );
}

#[test]
fn opting_out_of_extractor_errors_does_not_register_configured_bodies() {
    let app = Gotcha::new()
        .document_extractor_errors(false)
        .extractor_config(JsonConfig::default().error_handler(|_| (StatusCode::BAD_REQUEST, Problem { code: 1 })))
        .post("/json", json_body);
    let spec = serde_json::to_value(app.into_openapi()).unwrap();
    assert_eq!(statuses(&spec, "/json"), ["200"]);
    assert!(
        spec["components"]["schemas"].get("Problem").is_none(),
        "no response references the configured body"
    );
}

#[test]
fn explicit_declarations_win_over_automatic_responses() {
    let errors = one(operable(explicit_error), "/errors");
    assert_eq!(statuses(&errors, "/errors"), ["200", "400", "413", "415", "422"]);
    assert_eq!(responses(&errors, "/errors")["400"]["description"], "Explicit rejection");
    assert_eq!(
        responses(&errors, "/errors")["400"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Problem"
    );
    assert_eq!(error_body_ref(&responses(&errors, "/errors")["422"]), "#/components/schemas/ErrorBody");

    let responses_doc = one(operable(explicit_response), "/responses");
    assert_eq!(statuses(&responses_doc, "/responses"), ["200", "400", "413", "415", "422"]);
    assert_eq!(responses(&responses_doc, "/responses")["422"]["description"], "Explicit unprocessable");
    assert_eq!(
        responses(&responses_doc, "/responses")["422"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Problem"
    );
}

#[test]
fn providers_sharing_a_status_with_the_same_body_share_one_response() {
    let spec = one(operable(json_and_query), "/items");
    assert_eq!(statuses(&spec, "/items"), ["200", "400", "413", "415", "422"]);
    let bad_request = &responses(&spec, "/items")["400"];
    assert_eq!(bad_request["description"], "Request rejected: invalid query string / invalid JSON body");
    assert_eq!(error_body_ref(bad_request), "#/components/schemas/ErrorBody");
    assert!(bad_request["content"]["application/json"]["schema"].get("oneOf").is_none());
}

#[test]
fn providers_sharing_a_status_with_different_bodies_document_one_of() {
    let app = Gotcha::new()
        .extractor_config(QueryConfig::default().error_handler(|_| (StatusCode::BAD_REQUEST, Problem { code: 2 })))
        .post("/items", json_and_query);
    let spec = serde_json::to_value(app.into_openapi()).unwrap();
    let schema = &responses(&spec, "/items")["400"]["content"]["application/json"]["schema"];
    assert_eq!(
        schema,
        &json!({"oneOf": [
            {"$ref": "#/components/schemas/Problem"},
            {"$ref": "#/components/schemas/ErrorBody"}
        ]})
    );
}

#[test]
fn raw_axum_extractors_and_custom_providers_contribute_nothing() {
    for (op, path) in [(operable(raw_axum), "/raw"), (operable(raw_axum_form), "/raw-form")] {
        let spec = one(op, path);
        assert_eq!(statuses(&spec, path), ["200"], "raw Axum extractors add no rejection responses");
        assert!(spec["components"]["schemas"].get("ErrorBody").is_none());
    }

    struct Custom;
    impl ParameterProvider for Custom {}
    assert!(<Custom as ParameterProvider>::extractor_kinds().is_empty(), "user-defined providers are opt-in");
}

#[test]
fn a_declared_request_body_keeps_the_running_extractor_rejections() {
    let spec = one(operable(declared_body), "/items/{id}");
    assert_eq!(
        spec["paths"]["/items/{id}"]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/DeclaredBody"
    );
    assert_eq!(statuses(&spec, "/items/{id}"), ["200", "400", "413", "415", "422"]);
    assert_eq!(
        responses(&spec, "/items/{id}")["400"]["description"],
        "Request rejected: invalid path parameters / invalid query string / invalid JSON body"
    );
}

#[test]
fn skipped_arguments_contribute_neither_parameters_nor_rejections() {
    let spec = one(operable(skipped_json), "/skipped-json");
    assert_eq!(statuses(&spec, "/skipped-json"), ["200"]);
    assert!(spec["paths"]["/skipped-json"]["post"]["parameters"].as_array().unwrap().is_empty());
    assert!(spec["components"]["schemas"].get("ErrorBody").is_none());
}

#[test]
fn fully_overridden_rejections_leave_no_orphan_component() {
    let spec = one(operable(query_explicit), "/explicit-query");
    assert_eq!(statuses(&spec, "/explicit-query"), ["200", "400"]);
    assert_eq!(
        responses(&spec, "/explicit-query")["400"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/Problem"
    );
    assert!(spec["components"]["schemas"].get("Problem").is_some());
    assert!(
        spec["components"]["schemas"].get("ErrorBody").is_none(),
        "the shadowed default body must not be registered"
    );
}

#[test]
fn skip_response_replaces_return_type_inference_but_keeps_extractor_rejections() {
    let spec = one(operable(skipped), "/skipped");
    assert_eq!(statuses(&spec, "/skipped"), ["200", "400", "413", "415", "422"]);
    assert_eq!(responses(&spec, "/skipped")["200"]["content"]["text/plain"]["schema"]["type"], "string");
    assert_eq!(error_body_ref(&responses(&spec, "/skipped")["400"]), "#/components/schemas/ErrorBody");
}

#[test]
fn documents_can_opt_out_of_automatic_rejection_responses() {
    let exported = serde_json::to_value(Gotcha::new().document_extractor_errors(false).post("/json", json_body).into_openapi()).unwrap();
    assert_eq!(statuses(&exported, "/json"), ["200"]);
    assert!(exported["components"]["schemas"].get("ErrorBody").is_none());

    let router: GotchaRouter<()> = GotchaRouter::default().post("/json", json_body).document_extractor_errors(false);
    let installed = serde_json::to_value(router.into_openapi()).unwrap();
    assert_eq!(statuses(&installed, "/json"), ["200"]);

    struct OptedOut;
    impl GotchaApp for OptedOut {
        type State = ();
        type Config = EmptyConfig;

        async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
            Ok(())
        }

        fn openapi_endpoints(&self) -> Option<OpenApiEndpoints> {
            Some(OpenApiEndpoints::default())
        }

        fn document_extractor_errors(&self) -> bool {
            false
        }

        fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
            router.post("/json", json_body)
        }
    }

    assert_eq!(statuses(&serde_json::to_value(OptedOut.openapi_document()).unwrap(), "/json"), ["200"]);
}

#[tokio::test]
async fn the_trait_hook_opts_the_served_document_out_too() {
    struct OptedOut;
    impl GotchaApp for OptedOut {
        type State = ();
        type Config = EmptyConfig;

        async fn state(&self, _: &ConfigWrapper<EmptyConfig>) -> GotchaResult<()> {
            Ok(())
        }

        fn openapi_endpoints(&self) -> Option<OpenApiEndpoints> {
            Some(OpenApiEndpoints::default())
        }

        fn document_extractor_errors(&self) -> bool {
            false
        }

        fn routes(&self, router: GotchaRouter<GotchaContext<(), EmptyConfig>>) -> GotchaRouter<GotchaContext<(), EmptyConfig>> {
            router.post("/json", json_body)
        }
    }

    let router = OptedOut.build_router(GotchaContext::new((), ConfigWrapper::default())).await.unwrap();
    let response = router
        .clone()
        .oneshot(Request::builder().uri("/openapi.json").body(AxumBody::empty()).unwrap())
        .await
        .unwrap();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let spec: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(statuses(&spec, "/json"), ["200"]);
}

// These handlers match the fixture's source exactly; it was generated at the commit this feature
// branched from, so the document must not change.
#[derive(Clone, Debug, Deserialize, Serialize, Schematic)]
struct Body {
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Schematic)]
struct Out {
    id: u32,
}

#[api]
async fn raw_extractors(
    State(_): State<()>, _headers: HeaderMap, AxumPath(id): AxumPath<u32>, AxumQuery(_filter): AxumQuery<Filter>, AxumJson(_body): AxumJson<Body>,
) -> AxumJson<Out> {
    AxumJson(Out { id })
}

#[test]
fn operations_without_gotcha_providers_are_byte_identical_to_main() {
    let spec = document([
        ("/items/{id}", Method::POST, operable(raw_extractors)),
        ("/plain", Method::GET, operable(plain)),
    ]);
    let expected: Value = serde_json::from_str(include_str!("fixtures/no_provider_document.json")).unwrap();
    assert_eq!(spec, expected);
}
