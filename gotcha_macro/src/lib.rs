//! # Gotcha Macro
//!
//! Procedural macros for the Gotcha web framework, providing automatic OpenAPI schema generation
//! and enhanced route handling capabilities.
//!
//! ## Macros
//!
//! - `#[api]` - Generates OpenAPI documentation for route handlers
//! - `#[derive(Schematic)]` - Generates OpenAPI schemas for request/response types
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use gotcha::{api, Json, Path, Schematic};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Schematic, Serialize, Deserialize)]
//! struct User {
//!     id: u32,
//!     name: String,
//!     email: String,
//! }
//!
//! /// Get user by ID
//! #[api(id = "get_user", group = "users")]
//! async fn get_user(Path(id): Path<u32>) -> Json<User> {
//!     // Implementation here
//! }
//! ```

use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};

mod route;
mod schematic;

pub(crate) mod utils;

/// Generates OpenAPI documentation for route handler functions.
///
/// This attribute macro automatically generates OpenAPI operation specifications
/// based on the function signature, parameters, and return types.
///
/// ## Attributes
///
/// - `id` - Unique operation ID for the endpoint
/// - `group` - Group/tag for organizing operations in documentation
/// - `summary` - Human-readable operation summary (defaults to the id in Title Case)
/// - `deprecated` - Marks the operation as deprecated, e.g. `#[api(deprecated)]`
/// - `security` - Name of a required security scheme, e.g. `#[api(security = "bearerAuth")]`
/// - `responses(response(status = 404, body = "ApiError", description = "Not found"))` -
///   Explicit response declarations, replacing inferred entries for those statuses. `body`
///   is an optional Rust type string; `content_type` defaults to `application/json` for bodies.
///   `item = "Event"` declares the OpenAPI 3.2 `itemSchema` for sequential media types such as
///   `text/event-stream`, requires an explicit `content_type`, and is independent of `body`. For
///   `text/event-stream` it must describe the parsed event envelope (`data` plus the optional
///   `event`, `id`, and `retry` fields), not only JSON carried inside `data`.
/// - `request_body(content_type = "multipart/form-data", body = "UploadForm", description = "Upload", required = true)` -
///   Declare the request body instead of inferring it from extractor parameters. `body` is a
///   required Rust type string; `content_type` defaults to `application/json` (matching
///   `response(...)`) and `required` defaults to `true`. The declaration replaces the body
///   contributed by `Json`, `Form`, `Multipart`, and their wrappers; path, query, header, and
///   cookie parameters are unaffected. Repeat the entry with distinct `content_type` values to
///   document several representations of one body; the entries must agree on `description` and
///   `required`. Body types must implement `Schematic` and are registered as components exactly
///   like response bodies. A custom body extractor that leaves
///   `ParameterProvider::is_request_body()` at its default still has its replaced body type
///   registered as a component; the declaration still wins. Use `gotcha::openapi::Binary` for a
///   file field in a multipart form.
/// - `errors(response(status = 404, body = "ErrorBody", description = "Not found"))` -
///   Fully describe the `Err` branch of `Result<T, E>` (including type aliases). Requires at
///   least one declaration and only infers `T: Responsible`; `E` does not need `Responsible`.
///   Uses the same fields as `responses`. Error alternatives merge with the inferred success
///   contract; `responses(...)` overrides matching statuses afterward.
/// - `drop_default` - Remove the inferred `default` response. At least one response must remain.
/// - `skip_response` - Do not infer responses from the return type. The nonempty `responses(...)`
///   list describes the return-type part of the contract, and the return type is never placed in
///   a trait path, so opaque types such as `Sse<impl Stream<...>>` are accepted. Rejection
///   responses of Gotcha extractor arguments are still added, and explicit statuses still win.
///   Cannot be combined with `errors(...)` (declare every status in `responses(...)`) or
///   `drop_default` (nothing is inferred to drop).
/// - On a handler parameter, `#[api(skip)]` excludes that extractor from the documented
///   parameters and rejection responses. Its `ParameterProvider::extractor_kinds` is not
///   queried, so argument types without a `ParameterProvider` implementation still compile.
///   Declare any rejection the extractor can send with `errors(...)` or `responses(...)`. It
///   accepts exactly that form; other parameter-level `#[api(...)]` contents are compile errors.
///
/// ## Extractor rejection responses
///
/// Every documented argument that uses one of Gotcha's extractor providers (`Json`, `Form`,
/// `Query`, `Path`, `Multipart`, `TypedHeader`, `Header`, `Cookie`, or `Valid`) contributes the
/// rejection responses that provider can produce. The statuses come from the provider and the
/// body comes from the application's `XxxConfig` handler (`ErrorBody` by default); the document
/// assembly adds them at generation time. A status declared in `errors(...)` or `responses(...)`
/// wins over the automatic one. Axum's own extractors and custom `ParameterProvider`
/// implementations contribute nothing. Applications can opt out with
/// `Gotcha::document_extractor_errors(false)` or `GotchaApp::document_extractor_errors`.
///
/// Response and request body declarations describe HTTP behavior; they do not change what the
/// handler sends or reads. `errors(...)` preserves any `default` from the success branch unless
/// `drop_default` is explicit.
/// The error type still needs `IntoResponse` when the handler is registered on a router, and
/// declared body and item types must implement `Schematic`. Without `errors(...)` or
/// `skip_response`, the whole return type must implement `Responsible`, even when `responses(...)`
/// is present.
///
/// ## Example
///
/// ```rust,ignore
/// use gotcha::{api, Json, Path, Schematic};
///
/// #[derive(Schematic)]
/// struct User { id: u32, name: String }
///
/// /// Get user by ID
/// #[api(id = "get_user", group = "users")]
/// async fn get_user(Path(id): Path<u32>) -> Json<User> {
///     // Implementation
/// }
/// ```
///
/// Streaming handlers can declare their return-type contract explicitly:
///
/// ```rust,ignore
/// // For `text/event-stream`, `item` describes the parsed event, not the JSON in `data`.
/// #[api(
///     skip_response,
///     responses(response(status = 200, content_type = "text/event-stream", item = "ChatSseEvent"))
/// )]
/// async fn events() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
///     // Implementation
/// }
/// ```
///
/// A multipart form has no type for `Multipart` to infer, so declare the form it accepts:
///
/// ```rust,ignore
/// use gotcha::openapi::Binary;
/// use gotcha::{api, Multipart, Schematic};
///
/// #[derive(Schematic)]
/// struct UploadForm {
///     file: Binary,
///     title: String,
/// }
///
/// #[api(request_body(content_type = "multipart/form-data", body = "UploadForm"))]
/// async fn upload(_form: Multipart) -> String {
///     // Read the fields with `Multipart`; the declaration documents the wire format.
///     String::new()
/// }
/// ```
#[proc_macro_attribute]
pub fn api(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    route::request_handler(args, input_stream)
}

/// Derives OpenAPI schema generation for structs and enums.
///
/// This derive macro automatically implements the `Schematic` trait,
/// which generates OpenAPI JSON schemas for request and response types.
///
/// ## Dependencies
///
/// `Schematic` is defined in `gotcha_core`. Libraries can depend on that crate alone
/// and use `#[derive(gotcha_core::Schematic)]`; applications with the `openapi` feature
/// can use `#[derive(gotcha::Schematic)]` without adding a separate core dependency.
/// Neither form requires importing the trait into scope.
///
/// Generated paths prefer a direct `gotcha_core` dependency, then the core re-export
/// in `gotcha`. Cargo dependency renames are recognized for both. For a custom facade,
/// use `#[schematic(crate = "path::to::gotcha_core")]` to select its core re-export.
/// This overrides every generated support path, including field metadata.
///
/// ## Serde attributes
///
/// A named field's presence and nullability follow the serde attributes already on it, so the
/// schema matches what deserialization requires and serialization writes:
///
/// - `deserialize_with` or `with` without a `default` makes the field required, including
///   `Option<T>`. Combined with a recognized `Option::is_none` skip it is asymmetric (input
///   requires the key, output may omit it), so that case stays optional and nullable.
/// - `skip_serializing_if = "Option::is_none"` (also the `std::option::` and `core::option::`
///   paths) makes an `Option<T>` field optional and removes its null branch.
/// - `default` on the field or on a struct container (direct fields only; `flatten` fields keep
///   the inner type's requiredness), and any other `skip_serializing_if` predicate, make the
///   field optional.
///
/// `Option<Option<T>>` always produces a single null branch. The README section
/// "Optional and nullable fields" maps each pattern to its schema and zod equivalent.
///
/// ## Field attributes
///
/// Individual fields can be annotated with `#[schematic(...)]` to enrich the
/// generated schema with *documentation* metadata. All keys are optional:
/// `title`, `description`, `example`, `default`, `format`.
///
/// An explicit `description` overrides the field's doc comment, and `example` /
/// `default` preserve their JSON type (`example = 42` stays a number, not `"42"`).
/// The `example` attribute emits an OpenAPI 3.2 schema `examples` array.
///
/// Validation constraints belong in `#[validate(...)]` (the validator crate), not `#[schematic]`,
/// so a rule is written once. Where they map cleanly they are also reflected in the schema:
/// `range` → minimum/maximum, `length` → minLength/maxLength (minItems/maxItems for collections),
/// `email`/`url` → format.
///
/// ```rust,ignore
/// #[derive(Schematic, Serialize, Deserialize)]
/// struct CreateUserRequest {
///     #[schematic(description = "User's full name", example = "Ada Lovelace")]
///     name: String,
///     #[schematic(format = "email")]
///     email: String,
///     #[schematic(default = 18)]
///     age: u8,
/// }
/// ```
///
/// ## Example
///
/// ```rust,ignore
/// use gotcha::Schematic;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Schematic, Serialize, Deserialize)]
/// struct CreateUserRequest {
///     /// User's full name
///     name: String,
///     /// User's email address
///     email: String,
/// }
///
/// #[derive(Schematic, Serialize, Deserialize)]
/// enum UserType {
///     Admin,
///     Regular,
///     Guest,
/// }
/// ```
#[proc_macro_derive(Schematic, attributes(schematic))]
#[proc_macro_error]
pub fn derive_parameter(input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    match schematic::handler(stream2) {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err((span, msg)) => abort! {span, msg},
    }
}

/// Marks a struct as a Gotcha application state so it can be extracted directly
/// with axum's `State<T>` in handlers.
///
/// It generates `impl<C> FromRef<GotchaContext<T, C>> for T`, which
/// pulls the state out of the `GotchaContext` that the framework injects as the
/// axum state. Without this, handlers would have to extract the whole
/// `State<GotchaContext<T, C>>` and reach into `.state`.
///
/// The struct must be `Clone` and non-generic. (For a generic state, write the
/// `FromRef` impl by hand.)
///
/// ```ignore
/// use gotcha::prelude::*;
///
/// #[state]
/// #[derive(Clone, Default)]
/// struct AppState {
///     started_at: std::time::SystemTime,
/// }
///
/// async fn handler(State(state): State<AppState>) -> impl Responder {
///     format!("{:?}", state.started_at)
/// }
/// ```
/// Marks a struct as the application configuration so it can be extracted directly with
/// axum's `State<T>` in handlers.
///
/// It generates `impl FromRef<GotchaContext<S, Self>> for Self`, pulling the application's own
/// settings out of the loaded configuration. Without this, handlers would have to extract
/// `State<ConfigWrapper<Config>>` and reach through the wrapper.
///
/// The struct must be `Clone` and non-generic. Extraction itself does not require
/// `Serialize`, `Deserialize`, or `Default`.
///
/// ```ignore
/// use gotcha::prelude::*;
/// use serde::{Deserialize, Serialize};
///
/// #[config]
/// #[derive(Clone, Default, Serialize, Deserialize)]
/// struct Config {
///     name: String,
/// }
///
/// async fn handler(State(config): State<Config>) -> impl Responder {
///     config.name.clone()
/// }
/// ```
#[proc_macro_attribute]
pub fn config(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let ident = input.ident.clone();

    let generated = if input.generics.params.is_empty() {
        quote::quote! {
            impl<__GotchaState>
                ::gotcha::axum::extract::FromRef<::gotcha::GotchaContext<__GotchaState, #ident>> for #ident
            {
                fn from_ref(context: &::gotcha::GotchaContext<__GotchaState, #ident>) -> Self {
                    ::core::clone::Clone::clone(&context.config.app)
                }
            }
        }
    } else {
        syn::Error::new_spanned(
            &input.generics,
            "#[config] does not support generic structs; implement `FromRef<GotchaContext<S, Self>>` manually",
        )
        .to_compile_error()
    };

    quote::quote! {
        #input
        #generated
    }
    .into()
}

#[proc_macro_attribute]
pub fn state(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let ident = input.ident.clone();

    let generated = if input.generics.params.is_empty() {
        quote::quote! {
            impl<__GotchaConfig> ::gotcha::axum::extract::FromRef<::gotcha::GotchaContext<#ident, __GotchaConfig>> for #ident {
                fn from_ref(context: &::gotcha::GotchaContext<#ident, __GotchaConfig>) -> Self {
                    ::core::clone::Clone::clone(&context.state)
                }
            }
        }
    } else {
        syn::Error::new_spanned(
            &input.generics,
            "#[state] does not support generic structs; implement `FromRef<GotchaContext<Self, C>>` manually",
        )
        .to_compile_error()
    };

    quote::quote! {
        #input
        #generated
    }
    .into()
}
