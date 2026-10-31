// Every public item carries documentation, enforced rather than merely encouraged.
#![deny(missing_docs)]
// `doc(cfg(..))` is nightly-only; docs.rs sets `--cfg docsrs`, so the "requires feature X" badges
// appear there and are skipped elsewhere.
#![cfg_attr(docsrs, feature(doc_cfg))]
//! # Gotcha Core
//!
//! Lightweight schema core for the [Gotcha](https://github.com/Kilerd/gotcha/) web framework.
//!
//! This crate holds the [`Schematic`] trait, the [`EnhancedSchema`] type, and the
//! `Schematic` implementations for common data types. It intentionally depends only on
//! schema-related crates (no axum/tokio/tower), so a crate that only needs to derive and
//! use `Schematic` can depend on `gotcha_core` directly without pulling in the whole
//! web framework.
//!
//! The `gotcha` crate re-exports the core as `gotcha::gotcha_core` and exposes
//! `Schematic` at its root. Both entry points use this same trait; libraries can
//! implement it for their types without depending on the application framework.
//!
//! ```
//! #[derive(gotcha_core::Schematic)]
//! struct Record {
//!     id: u32,
//! }
//!
//! assert_eq!(<Record as gotcha_core::Schematic>::type_(), "object");
//! ```

// Keep generated absolute paths valid inside this crate as well as in downstream crates.
extern crate self as gotcha_core;

use std::collections::{BTreeMap, HashMap, HashSet};

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
/// Re-export of the [`Schematic`] derive macro. Kept alongside the trait of the same
/// name (different namespaces) so `use gotcha_core::Schematic;` brings in both.
pub use gotcha_macro::Schematic;
/// Re-export of the `oas` crate so macro-generated code can reference
/// `::gotcha_core::oas::` without a direct `oas` dependency at the derive site.
pub use oas;
use oas::Schema;
/// Re-export of `serde_json` so macro-generated code can reference
/// `::gotcha_core::serde_json::` without a direct `serde_json` dependency.
pub use serde_json;

pub mod registry;

pub mod request;
pub mod responsible;
pub use responsible::Responsible;

#[cfg(feature = "axum")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
pub mod parameter;
#[cfg(feature = "axum")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
pub use parameter::{ExtractorKind, ParameterProvider};

/// A schema plus whether the value it describes is required where it appears.
pub struct EnhancedSchema {
    /// The OpenAPI schema itself.
    pub schema: Schema,
    /// Whether the value must be present.
    pub required: bool,
}

/// One field in a type's [`Schematic::wire_fields`] table.
///
/// The table translates the Rust field names that `validator` reports into the names serde
/// reads from the wire, and carries the mapping of the field's own type so nesting composes.
/// [`Schematic`]'s derive emits it from the same `#[serde(..)]` attributes the schema uses;
/// types without named fields keep the empty default.
#[derive(Debug, Clone, Copy)]
pub struct WireField {
    /// The Rust field name, which is the key `validator` uses.
    pub rust: &'static str,
    /// The name serde accepts for this field.
    pub wire: &'static str,
    /// Whether the field is `#[serde(flatten)]`. A flattened field contributes no path segment:
    /// its own type's fields appear at the parent level.
    pub flatten: bool,
    /// The field type's own mapping. Container types delegate to the type they contain, so this
    /// is the mapping of the innermost named struct.
    pub nested: Option<fn() -> &'static [WireField]>,
}

/// Admit JSON null as an alternative to a schema, preserving all of its constraints.
///
/// `anyOf` also works for references, enums, and composed schemas, whose constraints
/// would still reject null if only their `type` keyword were widened. The referenced
/// component is never modified. Field presence remains [`EnhancedSchema::required`].
///
/// Idempotent: a schema that already has a `null` branch (for example the inner schema of
/// `Option<Option<T>>`) is returned unchanged, so nested options produce one `anyOf` with a
/// single `null` alternative rather than nested unions.
pub fn nullable_schema(mut schema: Schema) -> Schema {
    schema.nullable = None;
    let already_nullable = schema.extras.get("anyOf").and_then(serde_json::Value::as_array).is_some_and(|branches| {
        branches
            .iter()
            .any(|branch| branch.get("type").and_then(serde_json::Value::as_str) == Some("null"))
    });
    if already_nullable {
        return schema;
    }
    Schema {
        description: schema.description.take(),
        extras: std::collections::BTreeMap::from([("anyOf".into(), serde_json::json!([schema, {"type": "null"}]))]),
        ..Schema::default()
    }
}

/// Schematic is a trait that defines the schema of a type.
pub trait Schematic {
    /// The name of the type.
    fn name() -> &'static str;
    /// An explicit public component name, set by `#[schematic(name = "...")]`.
    /// Automatic names include generic arguments and are disambiguated during collection.
    fn schema_name() -> Option<&'static str> {
        None
    }
    /// Whether the type is required.
    fn required() -> bool;
    /// Whether the type admits null. The default generator emits a JSON Schema union.
    fn nullable() -> Option<bool> {
        None
    }
    /// The type of the type.
    fn type_() -> &'static str;
    /// The documentation of the type.
    fn doc() -> Option<String> {
        None
    }
    /// The format of the type.
    fn format() -> Option<String> {
        None
    }

    /// The type's fields, used to build object schemas and to flatten query parameters.
    fn fields() -> Vec<(&'static str, EnhancedSchema)> {
        vec![]
    }
    /// Generate the schema of the type.
    fn generate_schema() -> EnhancedSchema {
        let schema = Schema {
            _type: Some(Self::type_().into()),
            format: Self::format(),
            description: Self::doc(),
            ..Schema::default()
        };
        EnhancedSchema {
            schema: if Self::nullable() == Some(true) { nullable_schema(schema) } else { schema },
            required: Self::required(),
        }
    }
    /// Generate a schema suitable for flattening into another type.
    /// Returns None for types that should use fields() for flattening (structs),
    /// or Some(schema) for types that need special handling (enums with oneOf).
    fn flatten_schema() -> Option<serde_json::Value> {
        None
    }

    /// The Rust-name to serde-wire-name mapping for this type's fields.
    ///
    /// `validator` keys its errors by Rust field name, while clients read and write the names
    /// in `#[serde(rename = "..")]` / `#[serde(rename_all = "..")]`. gotcha's `Valid` extractor
    /// walks this table after validation to report failures under the names the client can act
    /// on; see `gotcha::validation::remap_to_wire_names`.
    ///
    /// The derive emits it for named structs. Container types (`Option`, `Vec`, maps, `Box`,
    /// `Arc`, arrays, ...) delegate to the type they contain, so a field's entry composes
    /// through any nesting. Types without named fields return the empty default, and a name
    /// that is not in the table is left alone.
    fn wire_fields() -> &'static [WireField] {
        &[]
    }
}

macro_rules! impl_primitive_type {
    ($t: ty, $name: expr, $api_type: expr) => {
        impl Schematic for $t {
            fn name() -> &'static str {
                $name
            }
            fn required() -> bool {
                true
            }
            fn type_() -> &'static str {
                $api_type
            }
        }
    };
}

impl_primitive_type! { i8, "i8", "integer"}
impl_primitive_type! { i16, "i16", "integer"}
impl_primitive_type! { i32, "i32", "integer"}
impl_primitive_type! { i64, "i64", "integer"}
impl_primitive_type! { isize, "isize", "integer"}
impl_primitive_type! { u8, "u8", "integer"}
impl_primitive_type! { u16, "u16", "integer"}
impl_primitive_type! { u32, "u32", "integer"}
impl_primitive_type! { u64, "u64", "integer"}
impl_primitive_type! { usize, "usize", "integer"}
impl_primitive_type! { String, "string", "string"}
impl_primitive_type! { bool, "bool", "boolean"}
impl_primitive_type! { f32, "f32", "number"}
impl_primitive_type! { f64, "f64", "number"}
impl_primitive_type! { i128, "i128", "integer"}
impl_primitive_type! { u128, "u128", "integer"}
impl_primitive_type! { char, "char", "string"}

impl Schematic for str {
    fn name() -> &'static str {
        "string"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}

/// A non-zero integer narrows `integer` to exclude zero. Unsigned types have `minimum: 1`;
/// signed types carry no bound because their valid range starts at their type minimum.
macro_rules! impl_nonzero_unsigned_type {
    ($t:ty, $name:expr) => {
        impl Schematic for $t {
            fn name() -> &'static str {
                $name
            }

            fn required() -> bool {
                true
            }

            fn type_() -> &'static str {
                "integer"
            }

            fn generate_schema() -> EnhancedSchema {
                let mut schema = EnhancedSchema {
                    schema: Schema {
                        _type: Some(Self::type_().into()),
                        format: None,
                        nullable: None,
                        description: Self::doc(),
                        extras: Default::default(),
                        ..Schema::default()
                    },
                    required: Self::required(),
                };
                schema.schema.extras.insert("minimum".to_string(), serde_json::json!(1));
                schema
            }
        }
    };
}

macro_rules! impl_nonzero_signed_type {
    ($t:ty, $name:expr) => {
        impl Schematic for $t {
            fn name() -> &'static str {
                $name
            }

            fn required() -> bool {
                true
            }

            fn type_() -> &'static str {
                "integer"
            }
        }
    };
}

impl_nonzero_unsigned_type! { std::num::NonZeroU8, "NonZeroU8" }
impl_nonzero_unsigned_type! { std::num::NonZeroU16, "NonZeroU16" }
impl_nonzero_unsigned_type! { std::num::NonZeroU32, "NonZeroU32" }
impl_nonzero_unsigned_type! { std::num::NonZeroU64, "NonZeroU64" }
impl_nonzero_unsigned_type! { std::num::NonZeroU128, "NonZeroU128" }
impl_nonzero_unsigned_type! { std::num::NonZeroUsize, "NonZeroUsize" }
impl_nonzero_signed_type! { std::num::NonZeroI8, "NonZeroI8" }
impl_nonzero_signed_type! { std::num::NonZeroI16, "NonZeroI16" }
impl_nonzero_signed_type! { std::num::NonZeroI32, "NonZeroI32" }
impl_nonzero_signed_type! { std::num::NonZeroI64, "NonZeroI64" }
impl_nonzero_signed_type! { std::num::NonZeroI128, "NonZeroI128" }
impl_nonzero_signed_type! { std::num::NonZeroIsize, "NonZeroIsize" }

impl Schematic for std::net::IpAddr {
    fn name() -> &'static str {
        "ip"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl Schematic for std::net::Ipv4Addr {
    fn name() -> &'static str {
        "ipv4"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("ipv4".to_string())
    }
}

impl Schematic for std::net::Ipv6Addr {
    fn name() -> &'static str {
        "ipv6"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("ipv6".to_string())
    }
}

/// A binary payload, such as a file field in a `multipart/form-data` form.
///
/// It documents as an OpenAPI 3.2 string carrying bytes:
/// `{"type": "string", "contentMediaType": "application/octet-stream"}`. The schema describes
/// the wire format only; parsing the upload is still the handler's job with `Multipart`.
///
/// `Binary` implements only `Schematic`: it has no serde implementation, so a documentation-only
/// form cannot derive `Serialize`/`Deserialize` and use field attributes such as
/// `skip_serializing_if`. In particular, an `Option<Binary>` field always documents as optional
/// and nullable.
///
/// ```rust
/// use gotcha_core::{Binary, Schematic};
///
/// #[derive(Schematic)]
/// struct UploadForm {
///     file: Binary,
///     title: String,
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Binary;

impl Schematic for Binary {
    fn name() -> &'static str {
        "binary"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn generate_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: Schema {
                _type: Some("string".into()),
                // OpenAPI 3.2 spells binary content as a string with a media type; the 3.0
                // `format: binary` keyword is not used.
                content_media_type: Some("application/octet-stream".into()),
                description: Self::doc(),
                ..Schema::default()
            },
            required: Self::required(),
        }
    }
}

/// The unit type means "no value". As a *return* type that is an empty body, which
/// [`Responsible`] documents as a response carrying no content. In the rare
/// case it appears as a schema (`Json<()>`, which serializes as `null`) it produces
/// `{"type":"null"}`.
impl Schematic for () {
    fn name() -> &'static str {
        "void"
    }

    fn required() -> bool {
        false
    }

    fn type_() -> &'static str {
        "null"
    }

    fn generate_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: Schema {
                _type: Some("null".into()),
                format: None,
                nullable: None,
                description: None,
                extras: Default::default(),
                ..Schema::default()
            },
            required: false,
        }
    }
}

impl Schematic for &str {
    fn name() -> &'static str {
        "string"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl Schematic for uuid::Uuid {
    fn name() -> &'static str {
        "uuid"
    }
    fn required() -> bool {
        true
    }
    fn format() -> Option<String> {
        Some("uuid".to_string())
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl Schematic for chrono::NaiveDateTime {
    fn name() -> &'static str {
        "datetime"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("date-time".to_string())
    }
}

impl Schematic for chrono::NaiveDate {
    fn name() -> &'static str {
        "date"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("date".to_string())
    }
}

impl Schematic for serde_json::Value {
    fn name() -> &'static str {
        "any"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> EnhancedSchema {
        // `serde_json::Value` is an arbitrary JSON value (object, array, string, number,
        // bool or null), so an empty schema (`{}`) — which matches anything — is the correct
        // representation. Emitting `{"type":"object"}` made client generators read it as
        // `Record<string, never>` (an empty object).
        EnhancedSchema {
            schema: Schema {
                _type: None,
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        }
    }
}

impl<T: Schematic> Schematic for Option<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        false
    }
    fn nullable() -> Option<bool> {
        Some(true)
    }

    fn type_() -> &'static str {
        T::type_()
    }

    fn doc() -> Option<String> {
        T::doc()
    }
    fn generate_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: nullable_schema(T::generate_schema().schema),
            required: Self::required(),
        }
    }
    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

impl<T: Schematic> Schematic for &T {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        T::required()
    }

    fn type_() -> &'static str {
        T::type_()
    }
    fn doc() -> Option<String> {
        T::doc()
    }
    fn generate_schema() -> EnhancedSchema {
        T::generate_schema()
    }
    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

/// Owned pointers and `Cow` are transparent: serde serializes the pointee, so the schema is the
/// pointee's schema.
macro_rules! impl_transparent_pointer {
    ($t:ty) => {
        impl<T> Schematic for $t
        where
            T: Schematic + ?Sized,
        {
            fn name() -> &'static str {
                T::name()
            }

            fn schema_name() -> Option<&'static str> {
                T::schema_name()
            }

            fn required() -> bool {
                T::required()
            }

            fn nullable() -> Option<bool> {
                T::nullable()
            }

            fn type_() -> &'static str {
                T::type_()
            }

            fn doc() -> Option<String> {
                T::doc()
            }

            fn format() -> Option<String> {
                T::format()
            }

            fn fields() -> Vec<(&'static str, EnhancedSchema)> {
                T::fields()
            }

            fn generate_schema() -> EnhancedSchema {
                T::generate_schema()
            }

            fn flatten_schema() -> Option<serde_json::Value> {
                T::flatten_schema()
            }

            fn wire_fields() -> &'static [WireField] {
                T::wire_fields()
            }
        }
    };
}

impl_transparent_pointer! { Box<T> }
impl_transparent_pointer! { std::sync::Arc<T> }
impl_transparent_pointer! { std::rc::Rc<T> }

impl<T> Schematic for std::borrow::Cow<'_, T>
where
    T: Schematic + ToOwned + ?Sized,
{
    fn name() -> &'static str {
        T::name()
    }

    fn schema_name() -> Option<&'static str> {
        T::schema_name()
    }

    fn required() -> bool {
        T::required()
    }

    fn nullable() -> Option<bool> {
        T::nullable()
    }

    fn type_() -> &'static str {
        T::type_()
    }

    fn doc() -> Option<String> {
        T::doc()
    }

    fn format() -> Option<String> {
        T::format()
    }

    fn fields() -> Vec<(&'static str, EnhancedSchema)> {
        T::fields()
    }

    fn generate_schema() -> EnhancedSchema {
        T::generate_schema()
    }

    fn flatten_schema() -> Option<serde_json::Value> {
        T::flatten_schema()
    }

    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

impl<T: Schematic> Schematic for Vec<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        // A `Vec<T>` field is always present (possibly as an empty array) unless
        // it is wrapped in `Option`; its requiredness must not depend on whether
        // the element type is required. Consistent with `HashSet`/`HashMap`.
        true
    }

    fn type_() -> &'static str {
        "array"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().into()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
    }
    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

impl<T: Schematic> Schematic for std::collections::VecDeque<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "array"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().into()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
    }

    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

impl<T: Schematic, const N: usize> Schematic for [T; N] {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "array"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().into()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        // A fixed-size array always has exactly N elements.
        schema.schema.extras.insert("minItems".to_string(), serde_json::json!(N));
        schema.schema.extras.insert("maxItems".to_string(), serde_json::json!(N));
        schema
    }

    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

impl Schematic for BigDecimal {
    fn name() -> &'static str {
        "string"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl Schematic for rust_decimal::Decimal {
    fn name() -> &'static str {
        "decimal"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("decimal".to_string())
    }
}

impl<T: Schematic> Schematic for HashSet<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "array"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().into()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
    }
    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

impl<T: Schematic> Schematic for std::collections::BTreeSet<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "array"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().into()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
    }

    fn wire_fields() -> &'static [WireField] {
        T::wire_fields()
    }
}

impl<K: ToString, V: Schematic> Schematic for HashMap<K, V> {
    fn name() -> &'static str {
        V::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().into()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        };
        schema
            .schema
            .extras
            .insert("additionalProperties".to_string(), V::generate_schema().schema.to_value());
        schema
    }
    fn wire_fields() -> &'static [WireField] {
        V::wire_fields()
    }
}

/// A `BTreeMap` documents exactly like a `HashMap`: an object whose values share one schema.
impl<K: ToString, V: Schematic> Schematic for BTreeMap<K, V> {
    fn name() -> &'static str {
        V::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().into()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
                ..Schema::default()
            },
            required: Self::required(),
        };
        schema
            .schema
            .extras
            .insert("additionalProperties".to_string(), V::generate_schema().schema.to_value());
        schema
    }
    fn wire_fields() -> &'static [WireField] {
        V::wire_fields()
    }
}

impl Schematic for DateTime<Utc> {
    fn name() -> &'static str {
        "datetime"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("date-time".to_string())
    }
}

impl Schematic for DateTime<chrono::FixedOffset> {
    fn name() -> &'static str {
        "datetime"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("date-time".to_string())
    }
}

impl Schematic for DateTime<chrono::Local> {
    fn name() -> &'static str {
        "datetime"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("date-time".to_string())
    }
}

impl Schematic for chrono::NaiveTime {
    fn name() -> &'static str {
        "time"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("time".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nullable_schemas_preserve_constraints_and_reference_siblings() {
        use serde_json::json;
        for original in [
            json!({"type": "string", "minLength": 1}),
            json!({"type": "string", "enum": ["ready", "done"]}),
            json!({"$ref": "#/components/schemas/Name", "maxLength": 10}),
            json!({"allOf": [{"type": "integer"}, {"minimum": 1}]}),
        ] {
            let mut schema: Schema = serde_json::from_value(original.clone()).unwrap();
            schema.description = Some("Optional value".into());
            schema.nullable = Some(true);
            assert_eq!(
                nullable_schema(schema).to_value(),
                json!({
                    "description": "Optional value",
                    "anyOf": [original, {"type": "null"}]
                })
            );
        }
        assert_eq!(<()>::generate_schema().schema.to_value(), json!({"type": "null"}));
    }

    #[test]
    fn nested_option_uses_a_single_null_branch() {
        use serde_json::json;
        assert_eq!(
            <Option<Option<String>> as Schematic>::generate_schema().schema.to_value(),
            json!({"anyOf": [{"type": "string"}, {"type": "null"}]})
        );
    }

    #[test]
    fn datetime_utc_has_date_time_format() {
        let schema = <DateTime<Utc> as Schematic>::generate_schema();
        assert_eq!(schema.schema.to_value()["type"], "string");
        assert_eq!(schema.schema.format.as_deref(), Some("date-time"));
    }

    #[test]
    fn vec_field_is_required_regardless_of_element() {
        // A bare Vec field is present (possibly empty), so it is required even
        // when the element type is not (previously delegated to `T::required`).
        assert!(<Vec<Option<u8>> as Schematic>::required());
        assert!(<Vec<u8> as Schematic>::required());
    }

    #[test]
    fn primitive_names_are_accurate() {
        assert_eq!(<i8 as Schematic>::name(), "i8");
        assert_eq!(<bool as Schematic>::name(), "bool");
        assert_eq!(<f32 as Schematic>::name(), "f32");
        assert_eq!(<f64 as Schematic>::name(), "f64");
    }

    #[test]
    fn binary_is_an_openapi_3_2_media_typed_string() {
        assert_eq!(
            <Binary as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string", "contentMediaType": "application/octet-stream"})
        );
    }

    #[test]
    fn json_value_is_an_empty_any_schema() {
        // `serde_json::Value` accepts any JSON, so its schema must be empty (`{}`), not
        // `{"type":"object"}` (which reads as `Record<string, never>`).
        let schema = <serde_json::Value as Schematic>::generate_schema();
        let value = serde_json::to_value(&schema.schema).unwrap();
        assert!(value.get("type").is_none(), "Value must not be typed: {value}");
        assert!(value.get("format").is_none(), "Value must not have a format: {value}");
    }

    #[derive(Clone, Schematic, serde::Deserialize, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Inner {
        #[allow(dead_code)]
        first_name: String,
    }

    #[test]
    fn pointers_and_cow_are_transparent() {
        let integer = serde_json::json!({"type": "integer"});
        assert_eq!(<Box<String> as Schematic>::name(), "string");
        assert_eq!(
            <Box<str> as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string"})
        );
        assert_eq!(<Box<u32> as Schematic>::generate_schema().schema.to_value(), integer);
        assert_eq!(<std::sync::Arc<u32> as Schematic>::generate_schema().schema.to_value(), integer);
        assert_eq!(<std::rc::Rc<u32> as Schematic>::generate_schema().schema.to_value(), integer);
        assert_eq!(<std::borrow::Cow<'_, str> as Schematic>::name(), "string");
        assert_eq!(<std::borrow::Cow<'_, Inner> as Schematic>::type_(), "object");
        assert_eq!(
            <std::borrow::Cow<'_, Inner> as Schematic>::generate_schema().schema.to_value()["properties"]["firstName"]["type"],
            "string"
        );
    }

    #[test]
    fn container_wire_fields_delegate_to_the_inner_struct() {
        fn wires<T: Schematic>() -> Vec<&'static str> {
            T::wire_fields().iter().map(|field| field.wire).collect()
        }

        assert_eq!(wires::<Inner>(), ["firstName"]);
        for wires_of in [
            wires::<Option<Inner>>,
            wires::<Vec<Inner>>,
            wires::<Box<Inner>>,
            wires::<std::sync::Arc<Inner>>,
            wires::<std::rc::Rc<Inner>>,
            wires::<std::borrow::Cow<'_, Inner>>,
            wires::<std::collections::BTreeSet<Inner>>,
            wires::<std::collections::VecDeque<Inner>>,
            wires::<std::collections::HashSet<Inner>>,
            wires::<[Inner; 2]>,
            wires::<std::collections::HashMap<String, Inner>>,
            wires::<BTreeMap<String, Inner>>,
        ] {
            assert_eq!(wires_of(), ["firstName"]);
        }
    }

    #[test]
    fn ip_types_document_as_strings_with_their_formats() {
        assert_eq!(
            <std::net::IpAddr as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string"})
        );
        assert_eq!(
            <std::net::Ipv4Addr as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string", "format": "ipv4"})
        );
        assert_eq!(
            <std::net::Ipv6Addr as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string", "format": "ipv6"})
        );
    }

    #[test]
    fn nonzero_integers_exclude_zero() {
        assert_eq!(
            <std::num::NonZeroU32 as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "integer", "minimum": 1})
        );
        assert_eq!(
            <std::num::NonZeroUsize as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "integer", "minimum": 1})
        );
        assert_eq!(
            <std::num::NonZeroI32 as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "integer"})
        );
    }

    #[test]
    fn chrono_time_types_document_their_formats() {
        assert_eq!(
            <DateTime<chrono::FixedOffset> as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string", "format": "date-time"})
        );
        assert_eq!(
            <DateTime<chrono::Local> as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string", "format": "date-time"})
        );
        assert_eq!(
            <chrono::NaiveTime as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "string", "format": "time"})
        );
    }

    #[test]
    fn sequences_and_arrays_document_their_items() {
        assert_eq!(
            <std::collections::BTreeSet<u8> as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "array", "items": {"type": "integer"}})
        );
        assert_eq!(
            <std::collections::VecDeque<u8> as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "array", "items": {"type": "integer"}})
        );
        // A fixed-size array always has exactly N elements.
        assert_eq!(
            <[u8; 3] as Schematic>::generate_schema().schema.to_value(),
            serde_json::json!({"type": "array", "items": {"type": "integer"}, "minItems": 3, "maxItems": 3})
        );
    }

    #[test]
    fn wide_and_character_primitives_are_typed() {
        assert_eq!(<char as Schematic>::type_(), "string");
        assert_eq!(<i128 as Schematic>::type_(), "integer");
        assert_eq!(<u128 as Schematic>::type_(), "integer");
        assert_eq!(<str as Schematic>::type_(), "string");
    }
}
