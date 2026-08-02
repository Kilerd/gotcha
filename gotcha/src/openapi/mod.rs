//! # OpenAPI Module
//!
//! This module provides OpenAPI documentation generation capabilities for Gotcha web applications.
//! It is enabled by default but can be disabled by opting out of the "openapi" feature.
//!
//! ## Features
//!
//! - Automatic OpenAPI spec generation from route definitions
//! - Support for operation parameters, request bodies, and responses
//! - Built-in Redoc and Scalar UI for API documentation viewing
//! - Grouping operations by tags
//! - Parameter validation and type information
//!
//! ## Example
//!
//! ```rust,no_run
//! use gotcha::{api, GotchaRouter};
//!
//! /// Get a user by id
//! #[api(id = "get_user", group = "users")]
//! async fn get_user() -> String {
//!     "user".to_string()
//! }
//!
//! fn routes(router: GotchaRouter) -> GotchaRouter {
//!     router.get("/users/{id}", get_user)
//! }
//! ```
//!
//! The generated spec is served at `/openapi.json`, with the Redoc UI at `/redoc`
//! and the Scalar UI at `/scalar` when the feature is enabled.

use std::collections::{BTreeMap, HashMap};

use axum::http::Method;
use axum::response::Html;
use convert_case::{Case, Casing};
use either::Either;
use oas::{Components, Info, OpenAPIV3, Operation, Parameter, PathItem, Referenceable, RequestBody, Responses, SecurityRequirement, Tag};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::Responder;

pub mod schematic;

/// Match a `{name}` path parameter.
///
/// Since axum 0.8 routes are written the way OpenAPI writes them — `/users/{id}` — so this no
/// longer rewrites anything. It stays as the single place that knows the syntax: [`crate::openapi`]
/// uses it to normalise a path, and `ParameterProvider` uses the same shape to pull out parameter
/// names. Before 0.8 axum used `:id`, and every path had to be translated here.
pub(crate) static PATH_VARIABLE_PATTERN: &str = r"\{([^}]+)\}";

pub(crate) async fn openapi_html() -> impl Responder {
    Html(include_str!("../../statics/redoc.html"))
}

pub(crate) async fn scalar_html() -> impl Responder {
    Html(include_str!("../../statics/scalar.html"))
}

/// What one handler argument contributes: either operation parameters or the request body.
pub type ParamType = Either<Vec<Parameter>, RequestBody>;

/// Builds an argument's [`ParamType`] given the route path (needed to name path parameters).
pub type ParamConstructor = Box<dyn Fn(String) -> ParamType + Sync + Send + 'static>;

/// Normalise a route path into the form OpenAPI uses for path templating.
///
/// Since axum 0.8 the two agree — a route is registered as `/users/{id}` and documented as
/// `/users/{id}` — so this is the identity. It is kept because the framework still owns the
/// question "what does a documented path look like", and because axum 0.7 needed a real
/// translation here (`:id` -> `{id}`).
pub fn replace_path_variable(path: String) -> String {
    path
}

/// The parameter names a route path captures, in order: `/users/{id}/books/{isbn}` -> `id`, `isbn`.
pub fn path_variable_names(path: &str) -> Vec<String> {
    let regex = Regex::new(PATH_VARIABLE_PATTERN).expect("path variable pattern is valid");
    regex.captures_iter(path).map(|caps| caps[1].to_string()).collect()
}

#[derive()]
/// Everything the `#[api]` macro records about one handler, collected via `inventory`.
pub struct Operable {
    /// Fully qualified name of the handler function, used to match a route to its `Operable`.
    pub type_name: &'static str,
    /// The operation id.
    pub id: &'static str,
    /// Tag the operation is grouped under.
    pub group: Option<&'static str>,
    /// Explicit summary; when absent it is derived from the id in Title Case.
    pub summary: Option<&'static str>,
    /// Description, taken from the handler's doc comment.
    pub description: Option<&'static str>,
    /// Whether the operation is marked deprecated.
    pub deprecated: bool,
    /// Name of a security scheme this operation requires.
    pub security: Option<&'static str>,
    /// One constructor per handler argument.
    pub parameters: &'static Lazy<Vec<ParamConstructor>>,
    /// Builds the operation's responses from the handler's return type.
    pub responses: &'static Lazy<Box<dyn Fn() -> Responses + Sync + Send + 'static>>,
}

impl Operable {
    /// Build the OpenAPI operation for this handler at `path`.
    pub fn generate(&self, path: String) -> Operation {
        let tags = self.group.map(|group| vec![group.to_string()]);
        let mut params = vec![];
        let mut request_body = None;
        for item in self.parameters.iter() {
            match item(path.clone()) {
                Either::Left(params_vec) => {
                    params.extend(params_vec.into_iter().map(|param| Referenceable::Data(param.clone())));
                }
                Either::Right(req_body) => request_body = Some(Referenceable::Data(req_body.clone())),
            }
        }
        let responses = (self.responses)();

        // An explicit `#[api(summary = "...")]` wins; otherwise derive it from the id in Title Case.
        let summary = self.summary.map(|s| s.to_string()).or_else(|| Some(self.id.to_case(Case::Title)));
        // `#[api(security = "scheme")]` requires that named scheme (with empty scopes) for this operation.
        let security = self.security.map(|scheme| {
            let mut data: BTreeMap<String, Vec<String>> = BTreeMap::new();
            data.insert(scheme.to_string(), vec![]);
            vec![SecurityRequirement { data }]
        });

        Operation {
            tags,
            summary,
            description: self.description.map(|v| v.to_string()),
            external_docs: None,
            operation_id: Some(self.id.to_string()),
            parameters: Some(params),
            request_body,
            responses,
            callbacks: None,
            deprecated: Some(self.deprecated),
            security,
            servers: None,
        }
    }
}

inventory::collect!(Operable);

/// Assemble the spec from the routes' [`Operable`] descriptors.
///
/// Every operation is generated inside a single [`registry::collect`](gotcha_core::registry::collect)
/// scope, so each named schema is emitted once under `components/schemas` and referenced by `$ref`
/// at its use sites (which is also what lets recursive types produce a finite spec).
pub fn generate_openapi(operables: HashMap<(String, Method), &'static Operable>) -> OpenAPIV3 {
    let (operations, schemas) = gotcha_core::registry::collect(|| {
        operables
            .into_iter()
            .map(|((path, method), operable)| {
                let operation = operable.generate(path.clone());
                ((path, method), operation)
            })
            .collect::<HashMap<(String, Method), Operation>>()
    });

    let components = (!schemas.is_empty()).then(|| Components {
        schemas: Some(schemas.into_iter().map(|(name, schema)| (name, Referenceable::Data(schema))).collect()),
        responses: None,
        parameters: None,
        examples: None,
        request_bodies: None,
        headers: None,
        security_schemes: None,
        links: None,
        callbacks: None,
    });

    let mut spec = OpenAPIV3 {
        info: Info {
            title: "Gotcha".to_string(),
            description: Some("Gotcha is a framework for building microservices".to_string()),
            terms_of_service: None,
            contact: None,
            license: None,
            version: "1.0.0".to_string(),
        },
        paths: BTreeMap::default(),
        servers: None,
        components,
        security: None,
        tags: None,
        openapi: "3.0.0".to_string(),
        external_docs: None,
        extras: None,
    };
    for ((path, method), operation) in operations {
        let path = replace_path_variable(path);
        if let Some(added_tags) = &operation.tags {
            added_tags.iter().for_each(|tag| {
                if let Some(tags) = &mut spec.tags {
                    if !tags.iter().any(|each| each.name.eq(tag)) {
                        tags.push(Tag::new(tag, None))
                    }
                }
            })
        }
        let entry = spec.paths.entry(path.to_string()).or_insert_with(|| PathItem {
            _ref: None,
            summary: None,
            description: None,
            get: None,
            put: None,
            post: None,
            delete: None,
            options: None,
            head: None,
            patch: None,
            trace: None,
            servers: None,
            parameters: None,
        });
        match method {
            Method::GET => entry.get = Some(operation),
            Method::POST => entry.post = Some(operation),
            Method::PUT => entry.put = Some(operation),
            Method::DELETE => entry.delete = Some(operation),
            Method::HEAD => entry.head = Some(operation),
            Method::OPTIONS => entry.options = Some(operation),
            Method::PATCH => entry.patch = Some(operation),
            Method::TRACE => entry.trace = Some(operation),
            _ => {}
        }
    }
    spec
}

#[cfg(all(test, feature = "openapi"))]
mod tests {
    use super::*;

    #[test]
    fn documented_paths_match_the_registered_route() {
        // axum 0.8 and OpenAPI write path parameters the same way, so a route is documented
        // exactly as it was registered.
        for path in ["/users", "/users/{id}", "/users/{id}/{name}", "/users/{userId}", "/items/{id2}/{ID}"] {
            assert_eq!(replace_path_variable(path.to_string()), path);
        }
    }

    #[test]
    fn path_variables_are_extracted_in_order() {
        assert_eq!(path_variable_names("/users/{id}/books/{isbn}"), ["id", "isbn"]);
        // Names are not restricted to lowercase.
        assert_eq!(path_variable_names("/items/{id2}/{ID}"), ["id2", "ID"]);
        assert!(path_variable_names("/users").is_empty());
    }
}
