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
//! The heavyweight `gotcha` crate re-exports everything here, so existing
//! `use gotcha::Schematic;` code keeps working unchanged.

use std::collections::{HashMap, HashSet};

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

pub mod responsible;
pub use responsible::Responsible;

#[cfg(feature = "axum")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
pub mod parameter;
#[cfg(feature = "axum")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
pub use parameter::ParameterProvider;

/// A schema plus whether the value it describes is required where it appears.
pub struct EnhancedSchema {
    /// The OpenAPI schema itself.
    pub schema: Schema,
    /// Whether the value must be present.
    pub required: bool,
}

/// Schematic is a trait that defines the schema of a type.
pub trait Schematic {
    /// The name of the type.
    fn name() -> &'static str;
    /// Whether the type is required.
    fn required() -> bool;
    /// Whether the type is nullable.
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
    /// Whether a value of this type is sent as an empty body. Only the unit type is, and
    /// [`Responsible`](crate::Responsible) documents it as a response with no content rather than
    /// as a JSON body.
    fn empty_body() -> bool {
        false
    }
    /// Generate the schema of the type.
    fn generate_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().to_string()),
                format: Self::format(),
                nullable: Self::nullable(),
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        }
    }
    /// Generate a schema suitable for flattening into another type.
    /// Returns None for types that should use fields() for flattening (structs),
    /// or Some(schema) for types that need special handling (enums with oneOf).
    fn flatten_schema() -> Option<serde_json::Value> {
        None
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

/// The unit type means "no value". As a *return* type that is an empty body, which
/// [`Responsible`](crate::Responsible) documents as a response carrying no content. In the rare
/// case it appears as a schema (`Json<()>`, which serializes as `null`) it produces an empty
/// schema — `"void"` is not a valid OpenAPI type and made the generated document invalid.
impl Schematic for () {
    fn name() -> &'static str {
        "void"
    }

    fn required() -> bool {
        false
    }

    fn type_() -> &'static str {
        "void"
    }

    fn empty_body() -> bool {
        true
    }

    fn generate_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: Schema {
                _type: None,
                format: None,
                nullable: None,
                description: None,
                extras: Default::default(),
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
        let enhanced_schema = T::generate_schema();
        let mut schema = enhanced_schema.schema;
        schema.nullable = Some(true);
        EnhancedSchema {
            schema,
            required: Self::required(),
        }
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
                _type: Some(Self::type_().to_string()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
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
                _type: Some(Self::type_().to_string()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
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
                _type: Some(Self::type_().to_string()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        };
        schema
            .schema
            .extras
            .insert("additionalProperties".to_string(), V::generate_schema().schema.to_value());
        schema
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime_utc_has_date_time_format() {
        let schema = <DateTime<Utc> as Schematic>::generate_schema();
        assert_eq!(schema.schema._type.as_deref(), Some("string"));
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
    fn json_value_is_an_empty_any_schema() {
        // `serde_json::Value` accepts any JSON, so its schema must be empty (`{}`), not
        // `{"type":"object"}` (which reads as `Record<string, never>`).
        let schema = <serde_json::Value as Schematic>::generate_schema();
        let value = serde_json::to_value(&schema.schema).unwrap();
        assert!(value.get("type").is_none(), "Value must not be typed: {value}");
        assert!(value.get("format").is_none(), "Value must not have a format: {value}");
    }
}
