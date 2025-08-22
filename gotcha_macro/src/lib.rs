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
//! ```rust
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
/// ```rust
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
/// ## Example
///
/// ```rust
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
