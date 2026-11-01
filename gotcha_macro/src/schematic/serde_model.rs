//! The serde attributes of a `#[derive(Schematic)]` input, parsed once with `serde_derive_internals`.
//!
//! serde's own attribute parser reads every form `#[derive(Serialize, Deserialize)]` accepts
//! (`rename = ".."` and `rename(serialize = .., deserialize = ..)`, `skip`, `with`, tagging, ...).
//! The schema generators read this model instead of parsing `#[serde(..)]` themselves.
//!
//! The generated schemas do not follow serde everywhere yet. The schema side honours only the
//! single-value `rename = ".."` / `rename_all = ".."` / `rename_all_fields = ".."` forms, a case
//! convention is applied by [`RenameAll`] to the identifier as written (`r#type` keeps its
//! prefix, `userID` is re-split into words) rather than as serde applies it, and the
//! `Option::is_none` check compares the predicate as written. The model records what those
//! need, so moving them to serde's reading is a change in this module.

use serde_derive_internals::attr::{self, RenameRule, TagType};
use serde_derive_internals::Ctxt;

use crate::utils::{get_serde_name, lit_str, option_inner_type, RenameAll};

/// How serde represents an enum.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Tagging {
    /// The default, externally tagged representation.
    External,
    /// `#[serde(tag = "type")]`.
    Internal(String),
    /// `#[serde(tag = "type", content = "data")]`.
    Adjacent { tag: String, content: String },
    /// `#[serde(untagged)]`.
    Untagged,
}

/// A `rename_all`-style case convention.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CaseRules {
    serialize: Option<RenameAll>,
    deserialize: Option<RenameAll>,
    /// Written as `key = ".."`, which sets both directions.
    single: bool,
}

impl CaseRules {
    fn new(rules: attr::RenameAllRules, single: bool) -> Self {
        CaseRules {
            serialize: rename_all(rules.serialize),
            deserialize: rename_all(rules.deserialize),
            single,
        }
    }

    /// The convention schema names use: only the single-value form.
    pub(crate) fn schema(self) -> Option<RenameAll> {
        self.single.then_some(self.serialize).flatten()
    }

    /// The convention serde deserializes with, in either form.
    pub(crate) fn deserialize(self) -> Option<RenameAll> {
        self.deserialize
    }
}

fn rename_all(rule: RenameRule) -> Option<RenameAll> {
    match rule {
        RenameRule::None => None,
        RenameRule::LowerCase => Some(RenameAll::LowerCase),
        RenameRule::UpperCase => Some(RenameAll::UpperCase),
        RenameRule::PascalCase => Some(RenameAll::PascalCase),
        RenameRule::CamelCase => Some(RenameAll::CamelCase),
        RenameRule::SnakeCase => Some(RenameAll::SnakeCase),
        RenameRule::ScreamingSnakeCase => Some(RenameAll::ScreamingSnakeCase),
        RenameRule::KebabCase => Some(RenameAll::KebabCase),
        RenameRule::ScreamingKebabCase => Some(RenameAll::ScreamingKebabCase),
    }
}

/// Container-level serde attributes.
#[derive(Debug)]
pub(crate) struct SerdeContainer {
    pub(crate) tagging: Tagging,
    /// `#[serde(default)]` or `#[serde(default = "..")]` on a struct.
    pub(crate) default: bool,
    pub(crate) transparent: bool,
    pub(crate) rename_all: CaseRules,
    /// Renames the fields of struct variants.
    pub(crate) rename_all_fields: CaseRules,
}

impl SerdeContainer {
    pub(crate) fn from_input(input: &syn::DeriveInput) -> Self {
        with_context(|cx| {
            let parsed = attr::Container::from_ast(cx, input);
            let spelling = Spelling::of(&input.attrs);
            SerdeContainer {
                tagging: match parsed.tag() {
                    TagType::External => Tagging::External,
                    TagType::Internal { tag } => Tagging::Internal(tag.clone()),
                    TagType::Adjacent { tag, content } => Tagging::Adjacent {
                        tag: tag.clone(),
                        content: content.clone(),
                    },
                    TagType::None => Tagging::Untagged,
                },
                default: !parsed.default().is_none(),
                transparent: parsed.transparent(),
                rename_all: CaseRules::new(parsed.rename_all_rules(), spelling.rename_all),
                rename_all_fields: CaseRules::new(parsed.rename_all_fields_rules(), spelling.rename_all_fields),
            }
        })
    }
}

/// Variant-level serde attributes.
#[derive(Debug, Default)]
pub(crate) struct SerdeVariant {
    /// An explicit serialize-side `rename`.
    rename: Option<String>,
    /// `rename` written as `rename = ".."`.
    rename_single: bool,
    /// Renames the fields of this struct variant.
    pub(crate) rename_all: CaseRules,
}

impl SerdeVariant {
    pub(crate) fn from_attrs(attrs: &[syn::Attribute]) -> Self {
        with_context(|cx| {
            let stand_in = syn::Variant {
                attrs: attrs.to_vec(),
                ident: stand_in_ident(),
                fields: syn::Fields::Unit,
                discriminant: None,
            };
            let parsed = attr::Variant::from_ast(cx, &stand_in);
            let spelling = Spelling::of(attrs);
            SerdeVariant {
                rename: explicit(parsed.name().serialize_name()),
                rename_single: spelling.rename,
                rename_all: CaseRules::new(parsed.rename_all_rules(), spelling.rename_all),
            }
        })
    }

    /// The name the schema documents for the variant `ident`, under the container's convention.
    pub(crate) fn schema_name(&self, ident: &str, rename_all: Option<RenameAll>) -> String {
        get_serde_name(ident, self.rename_single.then_some(self.rename.as_deref()).flatten(), rename_all)
    }
}

/// Field-level serde attributes.
#[derive(Debug, Default)]
pub(crate) struct SerdeField {
    /// An explicit `rename`, per direction.
    rename_serialize: Option<String>,
    rename_deserialize: Option<String>,
    /// `rename` written as `rename = ".."`.
    rename_single: bool,
    pub(crate) skip_serializing: bool,
    pub(crate) skip_deserializing: bool,
    pub(crate) flatten: bool,
    /// `default` or `default = ".."` on the field itself.
    default: bool,
    /// `deserialize_with` or `with`.
    deserialize_with: bool,
    skip_serializing_if: bool,
    /// The `skip_serializing_if` predicate is recognisably `Option::is_none`.
    skips_none: bool,
}

impl SerdeField {
    pub(crate) fn from_attrs(attrs: &[syn::Attribute]) -> Self {
        with_context(|cx| {
            // The unit type keeps serde from implying a `borrow` deserializer for `Cow` fields,
            // and a container default stops it from implying a field default for
            // `skip_deserializing`: only what the field states is recorded.
            let stand_in = syn::Field {
                attrs: attrs.to_vec(),
                vis: syn::Visibility::Inherited,
                mutability: syn::FieldMutability::None,
                ident: Some(stand_in_ident()),
                colon_token: None,
                ty: syn::parse_quote!(()),
            };
            let parsed = attr::Field::from_ast(cx, 0, &stand_in, None, &attr::Default::Default);
            let spelling = Spelling::of(attrs);
            SerdeField {
                rename_serialize: explicit(parsed.name().serialize_name()),
                rename_deserialize: explicit(parsed.name().deserialize_name()),
                rename_single: spelling.rename,
                skip_serializing: parsed.skip_serializing(),
                skip_deserializing: parsed.skip_deserializing(),
                flatten: parsed.flatten(),
                default: !parsed.default().is_none(),
                deserialize_with: parsed.deserialize_with().is_some(),
                skip_serializing_if: parsed.skip_serializing_if().is_some(),
                skips_none: parsed.skip_serializing_if().is_some() && spelling.skip_serializing_if.as_deref().is_some_and(is_option_is_none),
            }
        })
    }

    /// Whether serde drops the field in either direction (`skip`, `skip_serializing`,
    /// `skip_deserializing`).
    pub(crate) fn skipped(&self) -> bool {
        self.skip_serializing || self.skip_deserializing
    }

    /// The name the schema documents for the field `ident`, under the container's convention.
    pub(crate) fn schema_name(&self, ident: &str, rename_all: Option<RenameAll>) -> String {
        get_serde_name(ident, self.rename_single.then_some(self.rename_serialize.as_deref()).flatten(), rename_all)
    }

    /// The name serde *deserializes* the field `ident` from. Validation errors are reported under
    /// it, as it is the key a client actually sent.
    pub(crate) fn wire_name(&self, ident: &str, rename_all: Option<RenameAll>) -> String {
        get_serde_name(ident, self.rename_deserialize.as_deref(), rename_all)
    }

    /// Translate the serde attributes on a field of type `ty` into key presence and nullability.
    ///
    /// - A field-level or container-level `default` makes the key optional.
    /// - Otherwise `deserialize_with` / `with` makes it required, even for `Option<T>`: serde
    ///   rejects a missing key before the option can produce `None`.
    /// - Otherwise `skip_serializing_if` makes it optional, preserving the previous behavior.
    ///
    /// An `Option<T>` field skipped by a recognisable `Option::is_none` predicate emits `T`'s
    /// schema, dropping the null branch that serialization never writes. For `Option<Option<T>>`
    /// the inner option's null branch remains, keeping the tri-state.
    ///
    /// A custom deserializer plus such a skip and no `default` is asymmetric: input requires the
    /// key, output may omit it, and a single schema cannot express both. That combination keeps
    /// the previous optional + nullable schema.
    pub(crate) fn presence(&self, ty: &syn::Type, container_default: bool) -> SerdeFieldPresence {
        let has_default = self.default || container_default;
        let inner_option = option_inner_type(ty);
        let skips_none = inner_option.is_some() && self.skips_none;
        let asymmetric = self.deserialize_with && skips_none && !has_default;

        let required = if has_default || asymmetric {
            Some(false)
        } else if self.deserialize_with {
            Some(true)
        } else if self.skip_serializing_if {
            Some(false)
        } else {
            None
        };

        let schema_type = match inner_option {
            Some(inner) if skips_none && !asymmetric => inner.clone(),
            _ => ty.clone(),
        };

        SerdeFieldPresence { schema_type, required }
    }
}

/// How serde attributes on a named field translate into schema presence.
pub(crate) struct SerdeFieldPresence {
    /// The type whose schema to emit. This is the inner `T` when an `Option<T>` field skips
    /// `None`: the serialized value is then `T`, so the outer null branch is not in the contract.
    pub(crate) schema_type: syn::Type,
    /// `Some` when the attributes override the type's own `Schematic::required()`.
    pub(crate) required: Option<bool>,
}

/// Whether a `skip_serializing_if` predicate, as written, is recognisably `Option::is_none`.
///
/// Deliberately conservative: only the unqualified and the `std`/`core` paths, with optional
/// leading `::`, are recognized. An alias or a local wrapper predicate keeps the null branch.
fn is_option_is_none(predicate: &str) -> bool {
    let predicate = predicate.trim();
    let predicate = predicate.strip_prefix("::").unwrap_or(predicate);
    matches!(predicate, "Option::is_none" | "std::option::Option::is_none" | "core::option::Option::is_none")
}

/// Run `parse` with a serde error context. The errors are dropped: serde reports them from its
/// own derive on the same item.
fn with_context<T>(parse: impl FnOnce(&Ctxt) -> T) -> T {
    let cx = Ctxt::new();
    let parsed = parse(&cx);
    // A context must be checked before it is dropped.
    let _ = cx.check();
    parsed
}

/// The name a field or variant stand-in is given. serde reports the Rust name when nothing
/// renames it, so a name other than this one is an explicit `rename`.
const STAND_IN: &str = "__gotcha_stand_in";

fn stand_in_ident() -> syn::Ident {
    syn::Ident::new(STAND_IN, proc_macro2::Span::call_site())
}

fn explicit(name: &str) -> Option<String> {
    (name != STAND_IN).then(|| name.to_owned())
}

/// How the `#[serde(..)]` attributes are written, where the schema depends on more than their
/// meaning.
///
/// serde reads `rename = "x"` and `rename(serialize = "x", deserialize = "x")` alike, but the
/// schema side honours only the single-value form `key = ".."`. The `Option::is_none` check
/// compares the predicate as written rather than serde's parsed path.
#[derive(Default)]
struct Spelling {
    rename: bool,
    rename_all: bool,
    rename_all_fields: bool,
    skip_serializing_if: Option<String>,
}

impl Spelling {
    fn of(attrs: &[syn::Attribute]) -> Self {
        let mut spelling = Spelling::default();
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
            let Ok(items) = attr.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated) else {
                continue;
            };
            for item in items {
                let syn::Meta::NameValue(item) = item else {
                    continue;
                };
                let Some(value) = lit_str(&item.value) else {
                    continue;
                };
                if item.path.is_ident("rename") {
                    spelling.rename = true;
                } else if item.path.is_ident("rename_all") {
                    spelling.rename_all = true;
                } else if item.path.is_ident("rename_all_fields") {
                    spelling.rename_all_fields = true;
                } else if item.path.is_ident("skip_serializing_if") {
                    spelling.skip_serializing_if.get_or_insert(value);
                }
            }
        }
        spelling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(attrs: syn::Field) -> SerdeField {
        SerdeField::from_attrs(&attrs.attrs)
    }

    #[test]
    fn wire_names_read_the_deserialize_direction() {
        let item: syn::DeriveInput = syn::parse_quote! {
            struct Example {
                #[serde(rename(serialize = "wire-out", deserialize = "wire-in"))]
                directed: String,
                #[serde(rename = "both")]
                both: String,
                #[serde(rename(serialize = "wire-out"))]
                serialize_only: String,
                #[serde(rename = "renamed_to_itself")]
                renamed_to_itself: String,
            }
        };
        let syn::Data::Struct(data) = item.data else { unreachable!() };
        let fields: Vec<SerdeField> = data.fields.into_iter().map(field).collect();

        assert_eq!(fields[0].wire_name("directed", None), "wire-in");
        // A plain `rename` applies to both directions.
        assert_eq!(fields[1].wire_name("both", Some(RenameAll::UpperCase)), "both");
        // A serialize-only rename leaves deserialization on the Rust name.
        assert_eq!(fields[2].wire_name("serialize_only", Some(RenameAll::CamelCase)), "serializeOnly");
        // Renaming a field to its own name still opts it out of the case convention.
        assert_eq!(fields[3].wire_name("renamed_to_itself", Some(RenameAll::UpperCase)), "renamed_to_itself");
    }

    #[test]
    fn schema_names_honour_only_the_single_value_rename() {
        let item: syn::DeriveInput = syn::parse_quote! {
            struct Example {
                #[serde(rename = "both")]
                both: String,
                #[serde(rename(serialize = "out", deserialize = "out"))]
                listed: String,
            }
        };
        let syn::Data::Struct(data) = item.data else { unreachable!() };
        let fields: Vec<SerdeField> = data.fields.into_iter().map(field).collect();

        assert_eq!(fields[0].schema_name("both", Some(RenameAll::UpperCase)), "both");
        assert_eq!(fields[1].schema_name("listed", Some(RenameAll::UpperCase)), "LISTED");
    }

    #[test]
    fn case_conventions_keep_both_directions() {
        let container = |item: syn::DeriveInput| SerdeContainer::from_input(&item).rename_all;

        let rules = container(syn::parse_quote! {
            #[serde(rename_all(deserialize = "camelCase"))]
            struct Example { value: String }
        });
        assert_eq!(rules.deserialize(), Some(RenameAll::CamelCase));
        assert_eq!(rules.schema(), None);

        let rules = container(syn::parse_quote! {
            #[serde(rename_all = "kebab-case")]
            struct Example { value: String }
        });
        assert_eq!(rules.deserialize(), Some(RenameAll::KebabCase));
        assert_eq!(rules.schema(), Some(RenameAll::KebabCase));

        let rules = container(syn::parse_quote! {
            #[serde(rename_all(serialize = "camelCase"))]
            struct Example { value: String }
        });
        assert_eq!(rules.deserialize(), None);
        assert_eq!(rules.schema(), None);
    }

    #[test]
    fn only_stated_defaults_and_deserializers_count() {
        let item: syn::DeriveInput = syn::parse_quote! {
            struct Example<'a> {
                // serde implies a default for this field; the schema does not use it.
                #[serde(skip_deserializing)]
                implied_default: u8,
                #[serde(default = "path")]
                stated_default: u8,
                // serde implies a borrowing deserializer here; it is not a custom one.
                #[serde(borrow)]
                borrowed: Cow<'a, str>,
                #[serde(with = "module")]
                with_module: u8,
            }
        };
        let syn::Data::Struct(data) = item.data else { unreachable!() };
        let fields: Vec<SerdeField> = data.fields.into_iter().map(field).collect();
        let ty: syn::Type = syn::parse_quote!(u8);

        assert_eq!(fields[0].presence(&ty, false).required, None);
        assert_eq!(fields[1].presence(&ty, false).required, Some(false));
        assert_eq!(fields[2].presence(&ty, false).required, None);
        assert_eq!(fields[3].presence(&ty, false).required, Some(true));
    }

    #[test]
    fn option_is_none_predicates_are_recognized_as_written() {
        for (predicate, recognized) in [
            ("Option::is_none", true),
            (" ::std::option::Option::is_none ", true),
            ("core::option::Option::is_none", true),
            ("Option::is_some", false),
            ("my::Option::is_none", false),
            ("Option::<u8>::is_none", false),
            ("Option ::is_none", false),
        ] {
            assert_eq!(is_option_is_none(predicate), recognized, "{predicate}");
        }
    }

    #[test]
    fn tagging_follows_serde() {
        let tagging = |item: syn::DeriveInput| SerdeContainer::from_input(&item).tagging;
        assert_eq!(tagging(syn::parse_quote! { enum E { A } }), Tagging::External);
        assert_eq!(tagging(syn::parse_quote! { #[serde(tag = "t")] enum E { A } }), Tagging::Internal("t".into()));
        assert_eq!(
            tagging(syn::parse_quote! { #[serde(tag = "t", content = "c")] enum E { A } }),
            Tagging::Adjacent {
                tag: "t".into(),
                content: "c".into()
            }
        );
        assert_eq!(tagging(syn::parse_quote! { #[serde(untagged)] enum E { A } }), Tagging::Untagged);
    }
}
