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
//! use gotcha::{GotchaRouter, Operable};
//!
//! #[openapi(group = "Users")]
//! async fn get_user() -> impl Responder {
//!     // Handler implementation
//! }
//!
//! fn routes(router: GotchaRouter) -> GotchaRouter {
//!     router.get("/users/:id", get_user)
//! }
//! ```
//!
//! The OpenAPI documentation will be automatically generated and served at `/docs` 
//! and `/docs/scalar` endpoints when the feature is enabled.


use std::collections::{BTreeMap, HashMap};

use axum::response::Html;
use convert_case::{Case, Casing};
use either::Either;
use http::Method;
use oas::{Info, OpenAPIV3, Operation, Parameter, PathItem, Referenceable, RequestBody, Responses, Tag};
use once_cell::sync::Lazy;

use crate::Responder;


pub(crate) async fn openapi_html() -> impl Responder {
    Html(include_str!("../statics/redoc.html"))
}

pub(crate) async fn scalar_html() -> impl Responder {
    Html(include_str!("../statics/scalar.html"))
}

pub type ParamType = Either<Vec<Parameter>, RequestBody>;

pub type ParamConstructor = Box<dyn Fn(String) -> ParamType + Sync + Send + 'static>;

#[derive()]
pub struct Operable {
    pub type_name: &'static str,
    pub id: &'static str,
    pub group: Option<&'static str>,
    pub description: Option<&'static str>,
    pub deprecated: bool,
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
        let responses  = (self.responses)();
        
        Operation {
            tags,
            summary: Some(self.id.to_case(Case::Title)),
            description: self.description.map(|v| v.to_string()),
            external_docs: None,
            operation_id: Some(self.id.to_string()),
            parameters: Some(params),
            request_body,
            responses,
            callbacks: None,
            deprecated: Some(self.deprecated),
            security: None,
            servers: None,
        }
    }
}

inventory::collect!(Operable);

pub fn generate_openapi(operations: HashMap<(String, Method), Operation>) -> OpenAPIV3 {
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
        components: None,
        security: None,
        tags: None,
        openapi: "3.0.0".to_string(),
        external_docs: None,
        extras: None,
    };
    for ((path, method), operation) in operations {
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
