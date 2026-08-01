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
//!     router.get("/users/:id", get_user)
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

// Match `:name` path parameters up to the next `/`, mirroring axum (and
// `ParameterProvider`). The previous `[a-z_]`-only pattern skipped `:userId`,
// `:id2`, `:ID`, leaving them un-templated in the OpenAPI paths.
static PATH_VARIABLE_PATTERN: &str = r":[^/]+";

pub(crate) async fn openapi_html() -> impl Responder {
    Html(include_str!("../../statics/redoc.html"))
}

pub(crate) async fn scalar_html() -> impl Responder {
    Html(include_str!("../../statics/scalar.html"))
}

pub type ParamType = Either<Vec<Parameter>, RequestBody>;

pub type ParamConstructor = Box<dyn Fn(String) -> ParamType + Sync + Send + 'static>;

pub fn replace_path_variable(path: String) -> String {
    let regex = Regex::new(PATH_VARIABLE_PATTERN).unwrap();
    regex.replace_all(&path, |caps: &regex::Captures| format!("{{{}}}", &caps[0][1..])).to_string()
}

#[derive()]
pub struct Operable {
    pub type_name: &'static str,
    pub id: &'static str,
    pub group: Option<&'static str>,
    pub summary: Option<&'static str>,
    pub description: Option<&'static str>,
    pub deprecated: bool,
    pub security: Option<&'static str>,
    pub parameters: &'static Lazy<Vec<ParamConstructor>>,
    pub responses: &'static Lazy<Box<dyn Fn() -> Responses + Sync + Send + 'static>>,
}

impl Operable {
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
    fn test_path_variable_pattern() {
        assert_eq!(replace_path_variable("/users".to_string()), "/users");
        assert_eq!(replace_path_variable("/users/:id".to_string()), "/users/{id}");
        assert_eq!(replace_path_variable("/users/:id/:name".to_string()), "/users/{id}/{name}");
        // camelCase, digits and uppercase are now templated too (previously skipped)
        assert_eq!(replace_path_variable("/users/:userId".to_string()), "/users/{userId}");
        assert_eq!(replace_path_variable("/items/:id2/:ID".to_string()), "/items/{id2}/{ID}");
    }
}
