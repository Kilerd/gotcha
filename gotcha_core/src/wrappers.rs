//! Field wrappers that keep presence and nullability independent.
//!
//! [`Option<T>`] means optional *and* nullable at once. [`Nullable`] and [`Optional`] each
//! take one of those axes, matching zod's `.nullable()` and `.optional()`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::registry::SchemaReferences;
use crate::{nullable_schema, EnhancedSchema, Schematic};

/// A field that must be present but whose value may be `null`.
///
/// [`Option<T>`] is both optional and nullable at once. This wrapper keeps the two axes apart:
/// the key is required, but JSON `null` is a valid value, represented by `Nullable(None)`.
/// It is zod's `.nullable()` (`T | null`, not `T | undefined`).
///
/// A missing key is a deserialization error. The derived [`Deserialize`] goes through
/// `deserialize_newtype_struct`, which serde's `missing_field` deserializer forwards to
/// `deserialize_any` and rejects; only `deserialize_option` is answered with `None`.
/// Serialization always writes the key (`null` for `None`).
///
/// The generated schema is the same `anyOf: [<T>, {"type": "null"}]` shape used for
/// `Option<T>`, and the field stays in the parent object's `required` list.
///
/// # Caveats
///
/// Use it on ordinary struct fields. With `#[serde(flatten)]`, serde merges `T`'s keys into
/// the parent, but the generated schema does not follow the flatten and documents no
/// properties — the same pre-existing limitation as `Option<T>` with `flatten`.
///
/// The "a missing key is an error" guarantee assumes no `#[serde(default)]` is applied to the
/// field or to its container. A default (at either level) makes a missing key deserialize as
/// `Nullable(None)` — `Default` for this type — while the schema still lists the field as
/// required, because the derive only inspects field-level attributes.
///
/// # Example
///
/// ```
/// use gotcha_core::Nullable;
///
/// #[derive(gotcha_core::Schematic, serde::Deserialize)]
/// struct Patch {
///     /// Always present; `null` clears the value.
///     name: Nullable<String>,
/// }
///
/// let patch: Patch = serde_json::from_str(r#"{"name": null}"#).unwrap();
/// assert_eq!(patch.name, Nullable(None));
/// assert!(serde_json::from_str::<Patch>("{}").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nullable<T>(
    /// The value; `None` is serialized as JSON `null`.
    pub Option<T>,
);

impl<T> Default for Nullable<T> {
    /// The `null` value.
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Nullable<T> {
    /// Returns the wrapped value, consuming the wrapper.
    pub fn into_inner(self) -> Option<T> {
        self.0
    }

    /// Returns a reference to the wrapped value, or `None` when it is `null`.
    pub fn as_ref(&self) -> Option<&T> {
        self.0.as_ref()
    }

    /// Returns a mutable reference to the wrapped value, or `None` when it is `null`.
    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.0.as_mut()
    }

    /// Returns `true` when the value is `null`.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// Returns `true` when the value is present.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }
}

impl<T> From<Option<T>> for Nullable<T> {
    fn from(value: Option<T>) -> Self {
        Self(value)
    }
}

impl<T> From<T> for Nullable<T> {
    fn from(value: T) -> Self {
        Self(Some(value))
    }
}

impl<T: Schematic> Schematic for Nullable<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        true
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
}

impl<T: SchemaReferences> SchemaReferences for Nullable<T> {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        if let Some(value) = &mut self.0 {
            value.resolve_schema_references(names);
        }
    }
}

/// A field that may be absent but is never `null`.
///
/// This is zod's `.optional()` (`T | undefined`). Deserialization maps a missing key to
/// `Optional(None)` without `#[serde(default)]`. JSON `null` is also accepted as `None`
/// because deserialization delegates to [`Option::<T>::deserialize`]; the generated schema
/// does not advertise a null branch.
///
/// `None` serializes as `null` by default, so a response field that should be omitted needs
/// `#[serde(skip_serializing_if = "Optional::is_none")]` (the Schematic derive then also marks
/// it as not required).
///
/// The generated schema is exactly `T`'s schema, with the field left out of the parent
/// object's `required` list. `Optional<serde_json::Value>` is the optional opaque-JSON case;
/// no separate "any value" type is needed because `serde_json::Value` already documents as
/// the empty schema `{}`.
///
/// # Caveats
///
/// Use it on ordinary struct fields. With `#[serde(flatten)]`, serde merges `T`'s keys into
/// the parent, but the generated schema does not follow the flatten and documents no
/// properties — the same pre-existing limitation as `Option<T>` with `flatten`.
///
/// Because a missing key already deserializes as `Optional(None)`, field- or container-level
/// `#[serde(default)]` is unnecessary and has no additional effect on this field: its
/// `Default` is also `Optional(None)` and the field is already documented as not required.
///
/// # Example
///
/// ```
/// use gotcha_core::Optional;
///
/// #[derive(gotcha_core::Schematic, serde::Serialize, serde::Deserialize)]
/// struct Patch {
///     /// Absent means "unchanged"; the schema has no null branch.
///     #[serde(skip_serializing_if = "Optional::is_none")]
///     nickname: Optional<String>,
/// }
///
/// let patch: Patch = serde_json::from_str("{}").unwrap();
/// assert_eq!(patch.nickname, Optional(None));
/// assert_eq!(serde_json::to_string(&patch).unwrap(), "{}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Optional<T>(
    /// The value; `None` means the key is absent.
    pub Option<T>,
);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Optional<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Unlike `Nullable`, this must not go through `deserialize_newtype_struct`: serde's
        // private missing-field deserializer forwards that to `deserialize_any` (an error) and
        // only answers `deserialize_option` with `None`, which is exactly what `Option<T>`
        // requests. So a missing key deserializes as `Optional(None)` without `#[serde(default)]`.
        Option::<T>::deserialize(deserializer).map(Self)
    }
}

impl<T> Default for Optional<T> {
    /// The absent value.
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Optional<T> {
    /// Returns the wrapped value, consuming the wrapper.
    pub fn into_inner(self) -> Option<T> {
        self.0
    }

    /// Returns a reference to the wrapped value, or `None` when the field is absent.
    pub fn as_ref(&self) -> Option<&T> {
        self.0.as_ref()
    }

    /// Returns a mutable reference to the wrapped value, or `None` when the field is absent.
    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.0.as_mut()
    }

    /// Returns `true` when the field is absent. Use with
    /// `#[serde(skip_serializing_if = "Optional::is_none")]`.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// Returns `true` when the field is present.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }
}

impl<T> From<Option<T>> for Optional<T> {
    fn from(value: Option<T>) -> Self {
        Self(value)
    }
}

impl<T> From<T> for Optional<T> {
    fn from(value: T) -> Self {
        Self(Some(value))
    }
}

impl<T: Schematic> Schematic for Optional<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        false
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

    fn generate_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: T::generate_schema().schema,
            required: Self::required(),
        }
    }
}

impl<T: SchemaReferences> SchemaReferences for Optional<T> {
    fn resolve_schema_references(&mut self, names: &BTreeMap<String, String>) {
        if let Some(value) = &mut self.0 {
            value.resolve_schema_references(names);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct NullableField {
        value: Nullable<String>,
    }

    #[derive(Debug, Serialize)]
    struct NullableOutput {
        value: Nullable<String>,
    }

    #[derive(Debug, Deserialize)]
    struct OptionalField {
        value: Optional<String>,
    }

    #[derive(Debug, Serialize)]
    struct OptionalOutput {
        #[serde(skip_serializing_if = "Optional::is_none")]
        value: Optional<String>,
    }

    #[test]
    fn nullable_requires_the_key_and_accepts_null() {
        let error = serde_json::from_str::<NullableField>("{}").unwrap_err();
        assert!(error.to_string().contains("value"), "missing key must name the field: {error}");
        assert_eq!(serde_json::from_str::<NullableField>(r#"{"value": null}"#).unwrap().value, Nullable(None));
        assert_eq!(
            serde_json::from_str::<NullableField>(r#"{"value": "Ada"}"#).unwrap().value,
            Nullable(Some("Ada".into()))
        );

        assert_eq!(serde_json::to_string(&NullableOutput { value: Nullable(None) }).unwrap(), r#"{"value":null}"#);
        assert_eq!(
            serde_json::to_string(&NullableOutput {
                value: Nullable(Some("Ada".into()))
            })
            .unwrap(),
            r#"{"value":"Ada"}"#
        );
    }

    #[test]
    fn optional_allows_absence_and_accepts_null_leniently() {
        assert_eq!(serde_json::from_str::<OptionalField>("{}").unwrap().value, Optional(None));
        // `null` deserializes as `None` too; the schema does not advertise a null branch.
        assert_eq!(serde_json::from_str::<OptionalField>(r#"{"value": null}"#).unwrap().value, Optional(None));
        assert_eq!(
            serde_json::from_str::<OptionalField>(r#"{"value": "Ada"}"#).unwrap().value,
            Optional(Some("Ada".into()))
        );

        assert_eq!(serde_json::to_string(&OptionalOutput { value: Optional(None) }).unwrap(), "{}");
        assert_eq!(
            serde_json::to_string(&OptionalOutput {
                value: Optional(Some("Ada".into()))
            })
            .unwrap(),
            r#"{"value":"Ada"}"#
        );

        // Without `skip_serializing_if`, `None` is written as `null`.
        #[derive(Serialize)]
        struct Plain {
            value: Optional<String>,
        }
        assert_eq!(serde_json::to_string(&Plain { value: Optional(None) }).unwrap(), r#"{"value":null}"#);
    }

    #[test]
    fn wrappers_expose_the_option_axis() {
        assert!(Nullable::<u8>::default().is_none());
        assert_eq!(Nullable::from(1u8).into_inner(), Some(1));
        assert_eq!(Optional::from(Some(1u8)).as_ref(), Some(&1));
        assert_eq!(Optional::<u8>::from(None).into_inner(), None);
        assert!(Optional::from(1u8).is_some());
    }
}
