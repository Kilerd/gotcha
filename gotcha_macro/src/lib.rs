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
#[proc_macro_attribute]
pub fn api(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    route::request_handler(args, input_stream)
}

/// Derives OpenAPI schema generation for structs and enums.
///
/// This derive macro automatically implements the `Schematic` trait,
/// which generates OpenAPI JSON schemas for request and response types.
///
/// ## Field attributes
///
/// Individual fields can be annotated with `#[schematic(...)]` to enrich the
/// generated schema with *documentation* metadata. All keys are optional:
/// `title`, `description`, `example`, `default`, `format`.
///
/// An explicit `description` overrides the field's doc comment, and `example` /
/// `default` preserve their JSON type (`example = 42` stays a number, not `"42"`).
///
/// Validation constraints (min/max, length, regex, …) are intentionally *not* part of
/// `#[schematic]`; they are handled by request validation as a single source of truth, so
/// a constraint is never written twice.
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
/// It generates `impl<C: GotchaConfig> FromRef<GotchaContext<T, C>> for T`, which
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
#[proc_macro_attribute]
pub fn state(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let ident = input.ident.clone();

    let generated = if input.generics.params.is_empty() {
        quote::quote! {
            impl<__GotchaConfig: ::gotcha::GotchaConfig> ::gotcha::axum::extract::FromRef<::gotcha::GotchaContext<#ident, __GotchaConfig>> for #ident {
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
