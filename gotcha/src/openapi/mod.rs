//! # OpenAPI Module
//!
//! This module provides OpenAPI documentation generation capabilities for Gotcha web applications.
//! Enable the `openapi` feature to use it. HTTP documentation endpoints require
//! explicit application configuration through [`crate::Gotcha::with_openapi`] or
//! [`crate::GotchaApp::openapi_endpoints`].
//!
//! ## Features
//!
//! - Automatic OpenAPI spec generation from route definitions
//! - Support for operation parameters, request bodies, and responses
//! - Scalar UI for API documentation viewing
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
//! Generated documents use OpenAPI 3.2.0. The default endpoints are `/openapi.json` and `/scalar`.
//! The legacy Redoc UI is disabled by default because it does not support 3.2.
//! Use [`crate::GotchaRouter::into_openapi`] to export the document without HTTP.
//!
//! The document's `info` block defaults to neutral values (`title: "API"`, `version: "0.1.0"`,
//! no description). Set the application's identity with [`crate::Gotcha::openapi_info`],
//! [`crate::GotchaApp::openapi_info`], or [`crate::cargo_info!`]; `.openapi(..)` transforms still
//! run afterwards and can override it.

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::http::Method;
use convert_case::{Case, Casing};
use either::Either;
use oas::{
    Components, Info, MediaType, OpenAPIV3, Operation, Parameter, PathItem, Referenceable, RequestBody, Response, Responses, Schema, SchemaValue,
    SecurityRequirement, Tag,
};

mod endpoints;
pub use endpoints::OpenApiEndpoints;
pub mod schematic;
/// A binary payload, such as a file field in a `multipart/form-data` form.
pub use schematic::Binary;
pub(crate) mod transforms;

/// What one handler argument contributes: either operation parameters or the request body.
pub type ParamType = Either<Vec<Parameter>, RequestBody>;

/// Builds an argument's [`ParamType`] given the route path (needed to name path parameters).
/// A constructor is a function or a non-capturing closure; its result is built for each operation.
pub type ParamConstructor = fn(String) -> ParamType;

/// Reports the Gotcha extractor providers a handler argument uses, so document assembly can add
/// their rejection responses. Axum's originals and custom implementations report none.
pub type ExtractorConstructor = fn() -> &'static [crate::ExtractorKind];

/// Generate the parameters an extractor contributes, dropping any request body it would infer.
///
/// `#[api(request_body(...))]` uses this for every handler argument, so a declared body replaces
/// the inferred one while path, query, header, and cookie parameters are unchanged.
#[doc(hidden)]
pub fn parameters_only<T: crate::ParameterProvider>(url: String) -> ParamType {
    if T::is_request_body() {
        Either::Left(Vec::new())
    } else {
        T::generate(url)
    }
}

/// Everything the `#[api]` macro records about one handler, collected via `inventory`.
/// Constructor pointers are static; parameters, responses, and schemas are built per document.
///
/// Manual descriptors can use functions or non-capturing closures directly:
///
/// ```
/// use gotcha::{Operable, ParameterProvider, Path, Responsible};
///
/// static OPERATION: Operable = Operable {
///     type_name: "my_app::read_item",
///     id: "read_item",
///     group: None,
///     summary: None,
///     description: None,
///     deprecated: false,
///     security: None,
///     parameters: &[<Path<u32> as ParameterProvider>::generate],
///     extractors: &[<Path<u32> as ParameterProvider>::extractor_kinds],
///     responses: <String as Responsible>::response,
/// };
/// ```
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
    /// One constructor per documented handler argument, stored in a static slice.
    pub parameters: &'static [ParamConstructor],
    /// One constructor per documented handler argument reporting its Gotcha providers. Their
    /// rejection responses are added when the document is generated, using the application's
    /// extractor configurations.
    pub extractors: &'static [ExtractorConstructor],
    /// Builds fresh responses inside the current document's schema collection scope.
    pub responses: fn() -> Responses,
}

/// One rejection status shared by the providers of one operation.
#[derive(Default)]
struct RejectionGroup {
    /// Provider phrases, in handler-argument order, without duplicates.
    phrases: Vec<&'static str>,
    /// The distinct configured bodies that can produce this status.
    schemas: Vec<Schema>,
}

impl Operable {
    /// Build the OpenAPI operation for this handler at `path`.
    pub fn generate(&self, path: String) -> Operation {
        self.generate_with_configs(path, &crate::extract::ExtractorConfigs::default(), true)
    }

    /// Build the operation, consulting the application's extractor configurations.
    ///
    /// `document_extractor_errors` is the application-level opt-out. Explicit statuses already
    /// present in `responses` win over the automatic ones.
    pub(crate) fn generate_with_configs(&self, path: String, configs: &crate::extract::ExtractorConfigs, document_extractor_errors: bool) -> Operation {
        let tags = self.group.map(|group| vec![group.to_string()]);
        let mut params = vec![];
        let mut request_body = None;
        for item in self.parameters.iter() {
            match item(path.clone()) {
                Either::Left(params_vec) => {
                    params.extend(params_vec.into_iter().map(Referenceable::Data));
                }
                Either::Right(req_body) => request_body = Some(Referenceable::Data(req_body)),
            }
        }
        let mut responses = (self.responses)();
        if document_extractor_errors {
            for (status, response) in rejection_responses(self.extractors, configs, &responses).data {
                responses.data.entry(status).or_insert(response);
            }
        }

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

/// Collect the rejection responses of every provider the operation's arguments use.
///
/// Providers can share a status. Identical configured bodies produce one response; distinct
/// bodies become `oneOf` alternatives under `application/json`. A status that already has an
/// explicit or inferred response is skipped before its body schema is built, so a fully
/// overridden operation leaves no unreferenced component behind.
fn rejection_responses(extractors: &'static [ExtractorConstructor], configs: &crate::extract::ExtractorConfigs, existing: &Responses) -> Responses {
    let mut groups: BTreeMap<u16, RejectionGroup> = BTreeMap::new();
    for constructor in extractors {
        for kind in constructor() {
            let spec = configs.rejection_spec(*kind);
            for (status, phrase) in spec.statuses {
                if existing.data.contains_key(&status.as_u16().to_string()) {
                    continue;
                }
                let group = groups.entry(status.as_u16()).or_default();
                if !group.phrases.contains(&phrase) {
                    group.phrases.push(phrase);
                }
                if !crate::error::status_has_body(status) {
                    continue;
                }
                let schema = (spec.body_schema)().schema;
                let value = serde_json::to_value(&schema).expect("schemas are serializable");
                // The same configured body type generates the same reference, so only distinct
                // bodies become alternatives.
                if !group
                    .schemas
                    .iter()
                    .any(|each| serde_json::to_value(each).expect("schemas are serializable") == value)
                {
                    group.schemas.push(schema);
                }
            }
        }
    }
    Responses {
        default: None,
        data: groups
            .into_iter()
            .map(|(status, group)| (status.to_string(), Referenceable::Data(rejection_response(status, group))))
            .collect(),
    }
}

fn rejection_response(status: u16, group: RejectionGroup) -> Response {
    let description = format!("Request rejected: {}", group.phrases.join(" / "));
    let carries_body = axum::http::StatusCode::from_u16(status).is_ok_and(crate::error::status_has_body);
    let content = carries_body.then(|| {
        let schema: SchemaValue = if group.schemas.len() == 1 {
            group.schemas.into_iter().next().expect("at least one body").into()
        } else {
            Schema {
                extras: BTreeMap::from([(
                    "oneOf".to_owned(),
                    serde_json::Value::Array(
                        group
                            .schemas
                            .into_iter()
                            .map(|schema| serde_json::to_value(schema).expect("schemas are serializable"))
                            .collect(),
                    ),
                )]),
                ..Schema::default()
            }
            .into()
        };
        BTreeMap::from([(
            "application/json".to_owned(),
            MediaType {
                schema: Some(schema),
                example: None,
                examples: None,
                encoding: None,
                item_schema: None,
            },
        )])
    });
    Response {
        description,
        headers: None,
        content,
        links: None,
    }
}

/// The neutral [`Info`] used when an application does not configure one.
///
/// It carries no framework branding: `title` is `"API"`, `version` is `"0.1.0"`, and every
/// optional field, including `description`, is `None`. Applications set their own identity with
/// [`Gotcha::openapi_info`](crate::Gotcha::openapi_info) or
/// [`GotchaApp::openapi_info`](crate::GotchaApp::openapi_info), commonly through
/// [`cargo_info!`](crate::cargo_info).
pub(crate) fn default_info() -> Info {
    Info {
        title: "API".to_string(),
        description: None,
        terms_of_service: None,
        contact: None,
        license: None,
        version: "0.1.0".to_string(),
    }
}

/// Builds an [`oas::Info`] from the calling crate's Cargo package metadata.
///
/// `title` comes from `CARGO_PKG_NAME`, `version` from `CARGO_PKG_VERSION`, and `description`
/// from `CARGO_PKG_DESCRIPTION`, omitting the description when it is empty. Every other field is
/// `None`. The macro expands in the caller's crate, so it reports the application's package
/// rather than gotcha's.
///
/// Pass it to [`Gotcha::openapi_info`](crate::Gotcha::openapi_info) or return it from
/// [`GotchaApp::openapi_info`](crate::GotchaApp::openapi_info):
///
/// ```
/// # #[cfg(feature = "openapi")]
/// # {
/// let app = gotcha::Gotcha::new().openapi_info(gotcha::cargo_info!());
/// assert_eq!(app.into_openapi().info.title, env!("CARGO_PKG_NAME"));
/// # }
/// ```
#[macro_export]
macro_rules! cargo_info {
    () => {
        $crate::oas::Info {
            title: env!("CARGO_PKG_NAME").to_string(),
            description: {
                let description: &str = env!("CARGO_PKG_DESCRIPTION");
                if description.is_empty() {
                    None
                } else {
                    Some(description.to_string())
                }
            },
            terms_of_service: None,
            contact: None,
            license: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    };
}

/// The order [`PathItem`] writes methods in, which is the order they appear in the document.
fn document_method_order(method: &Method) -> usize {
    match method.as_str() {
        "GET" => 0,
        "PUT" => 1,
        "POST" => 2,
        "DELETE" => 3,
        "OPTIONS" => 4,
        "HEAD" => 5,
        "PATCH" => 6,
        "TRACE" => 7,
        _ => usize::MAX,
    }
}

/// Give every operation a unique `operationId`.
///
/// The default id is the handler's function name, so `users::list` and `books::list` — or one
/// handler mounted at two paths — would otherwise emit the same id and collide in generated
/// clients. Operations are visited by path and in the method order the document uses: the first
/// one keeps its id and each later duplicate gets `_2`, `_3`, ... in that order. Every operation's
/// original id is reserved, so a generated suffix never renames an id an operation declares
/// itself. Each rename is logged with both routes.
fn disambiguate_operation_ids(operables: &[((String, Method), &'static Operable)], operations: &mut [Operation]) {
    let original_ids: HashSet<&str> = operables.iter().map(|(_, operable)| operable.id).collect();
    let mut claimed: HashMap<String, (String, Method)> = HashMap::new();
    let mut order: Vec<usize> = (0..operables.len()).collect();
    order.sort_by(|&left, &right| {
        let ((left_path, left_method), _) = &operables[left];
        let ((right_path, right_method), _) = &operables[right];
        (left_path, document_method_order(left_method)).cmp(&(right_path, document_method_order(right_method)))
    });
    for index in order {
        let ((path, method), _) = &operables[index];
        let operation = &mut operations[index];
        let Some(id) = operation.operation_id.clone() else {
            continue;
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = claimed.entry(id.clone()) {
            entry.insert((path.clone(), method.clone()));
            continue;
        }
        let mut suffix = 2;
        let unique = loop {
            let candidate = format!("{id}_{suffix}");
            suffix += 1;
            if !original_ids.contains(candidate.as_str()) && !claimed.contains_key(&candidate) {
                break candidate;
            }
        };
        let (first_path, first_method) = claimed.get(&id).expect("a duplicate has a first operation");
        tracing::warn!(
            operation_id = id.as_str(),
            first_route = %format!("{} {}", first_method.as_str(), first_path),
            duplicate_route = %format!("{} {}", method.as_str(), path),
            renamed_to = unique.as_str(),
            "duplicate OpenAPI operationId; renamed the later operation"
        );
        operation.operation_id = Some(unique.clone());
        claimed.insert(unique, (path.clone(), method.clone()));
    }
}

/// Assemble the spec from the routes' [`Operable`] descriptors.
///
/// Every operation is generated inside a single [`registry::collect`](gotcha_core::registry::collect)
/// scope, so each named schema is emitted once under `components/schemas` and referenced by `$ref`
/// at its use sites (which is also what lets recursive types produce a finite spec).
///
/// Operation ids are made unique. A handler defaults to its function name, so two modules' `list`
/// functions — or one handler mounted at two paths — would otherwise share an id. Operations are
/// visited by path and in the method order the document uses (GET, PUT, POST, ...): the first
/// keeps its id and each later duplicate gets `_2`, `_3`, ... in that order. Explicit
/// `#[api(id = "...")]` ids follow the same rule.
pub fn generate_openapi(operables: HashMap<(String, Method), &'static Operable>) -> OpenAPIV3 {
    let configs = crate::extract::ExtractorConfigs::default();
    generate_openapi_with_configs_and_info(operables, &configs, true, default_info())
}

/// Like [`generate_openapi`], with the application's extractor configurations and opt-out.
///
/// Every operation documents the rejection responses of the Gotcha providers its arguments use.
/// A configured error body is built inside the collection scope only when one of those responses
/// references it, so overrides that no documented operation reaches leave no component behind.
#[doc(hidden)]
pub fn generate_openapi_with_configs_and_info(
    operables: HashMap<(String, Method), &'static Operable>, configs: &crate::extract::ExtractorConfigs, document_extractor_errors: bool, info: Info,
) -> OpenAPIV3 {
    let mut operables: Vec<_> = operables.into_iter().collect();
    operables.sort_by(|((path_a, method_a), _), ((path_b, method_b), _)| (path_a, method_a.as_str()).cmp(&(path_b, method_b.as_str())));
    let (mut operations, schemas) = gotcha_core::registry::collect(|| {
        operables
            .iter()
            .map(|((path, _), operable)| operable.generate_with_configs(path.clone(), configs, document_extractor_errors))
            .collect::<Vec<_>>()
    });
    disambiguate_operation_ids(&operables, &mut operations);

    let components = (!schemas.is_empty()).then(|| Components {
        schemas: Some(schemas.into_iter().map(|(name, schema)| (name, schema.into())).collect()),
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
        info,
        paths: BTreeMap::default(),
        servers: None,
        components,
        security: None,
        tags: None,
        openapi: "3.2.0".to_string(),
        external_docs: None,
        extras: None,
    };
    let mut document_tags: Vec<Tag> = vec![];
    for (((path, method), _), operation) in operables.into_iter().zip(operations) {
        // Declare every operation tag at the document level, once, in first-seen order. The
        // operations were sorted by path and method above, so the document order is stable.
        if let Some(operation_tags) = &operation.tags {
            operation_tags.iter().for_each(|tag| {
                if !document_tags.iter().any(|each| each.name.eq(tag)) {
                    document_tags.push(Tag::new(tag, None::<String>))
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
    // Documents with no grouped operations stay without a `tags` array rather than gaining an
    // empty one.
    spec.tags = (!document_tags.is_empty()).then_some(document_tags);
    spec
}
