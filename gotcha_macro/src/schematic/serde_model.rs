//! The serde attributes of a `#[derive(Schematic)]` input, parsed once with `serde_derive_internals`.
//!
//! serde's own attribute parser reads every form `#[derive(Serialize, Deserialize)]` accepts
//! (`rename = ".."` and `rename(serialize = .., deserialize = ..)`, `skip`, `with`, tagging, ...).
//! The schema generators read this model instead of parsing `#[serde(..)]` themselves.
//!
//! Names follow serde's rules: the identifier without its raw prefix, under serde's own
//! `rename_all` conventions. A type has one schema, so it cannot describe both directions where
//! their names differ; [`schema_name`] says which one it documents. `Valid` errors and query and
//! path parameters use the deserialize-side name, which is what a client sends.
//!
//! serde's parser stops reading a `#[serde(..)]` list at a key it does not know, where the
//! previous readers skipped it, so a key newer than this parser would hide the items after it in
//! the same list. `serde_derive_internals` 0.29.1 knows the same keys as `serde_derive` 1.0.229.

use std::collections::BTreeSet;

use serde_derive_internals::attr::{self, RenameAllRules, RenameRule, TagType};
use serde_derive_internals::Ctxt;
use syn::ext::IdentExt;

use crate::utils::option_inner_type;

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

/// A `rename_all`-style convention.
#[derive(Clone, Copy)]
pub(crate) struct CaseRules {
    directions: RenameAllRules,
    /// The rule when both directions follow the same one.
    common: RenameRule,
}

impl CaseRules {
    fn new(directions: RenameAllRules) -> Self {
        let common = if directions.serialize == directions.deserialize {
            directions.serialize
        } else {
            RenameRule::None
        };
        CaseRules { directions, common }
    }

    fn or(self, other: Self) -> Self {
        CaseRules {
            directions: self.directions.or(other.directions),
            common: self.common.or(other.common),
        }
    }
}

/// The one name a schema documents for what serde writes as `serialize` and reads as `deserialize`.
///
/// When the two differ, one schema cannot describe both directions. `common` is the identifier
/// under a convention that ignores the directions. It stays when it is what serde reads, so that
/// direction keeps working. Otherwise the serialize-side name applies: it is either `common`
/// itself or makes the output right where `common` is wrong in both directions.
fn schema_name(serialize: &str, deserialize: &str, common: String) -> String {
    if common == deserialize {
        common
    } else {
        serialize.to_owned()
    }
}

/// Container-level serde attributes.
pub(crate) struct SerdeContainer {
    /// The name serde writes for the container, the value of an internally tagged struct's tag.
    pub(crate) serialize_name: String,
    /// `from`, `try_from` or `into`: serde writes or reads another type instead of this one.
    pub(crate) converts: bool,
    /// `from` or `try_from`: serde reads another type, so this one's names are not what it reads.
    pub(crate) reads_another_type: bool,
    /// The type that documents the container, when serde writes and reads it for the container:
    /// `into` naming the same type as `from` or `try_from`. Any other conversion leaves the
    /// container's own fields, because they are what a direction uses or one schema cannot say both.
    pub(crate) proxy: Option<syn::Type>,
    pub(crate) tagging: Tagging,
    /// `#[serde(default)]` or `#[serde(default = "..")]` on a struct.
    pub(crate) default: bool,
    pub(crate) transparent: bool,
    pub(crate) deny_unknown_fields: bool,
    /// Names a struct's fields or an enum's variants.
    pub(crate) rename_all: CaseRules,
    /// Names the fields of struct variants.
    rename_all_fields: CaseRules,
}

impl SerdeContainer {
    /// Whether `deny_unknown_fields` decides what serde writes and reads. A conversion replaces
    /// them with another type, whose keys this one's schema cannot speak for.
    pub(crate) fn closes_objects(&self) -> bool {
        self.deny_unknown_fields && !self.converts
    }

    pub(crate) fn from_input(input: &syn::DeriveInput) -> Self {
        with_context(|cx| {
            let parsed = attr::Container::from_ast(cx, input);
            SerdeContainer {
                serialize_name: parsed.name().serialize_name().to_owned(),
                converts: parsed.type_from().is_some() || parsed.type_try_from().is_some() || parsed.type_into().is_some(),
                reads_another_type: parsed.type_from().is_some() || parsed.type_try_from().is_some(),
                proxy: parsed
                    .type_into()
                    .filter(|into| parsed.type_from() == Some(*into) || parsed.type_try_from() == Some(*into))
                    .cloned(),
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
                deny_unknown_fields: parsed.deny_unknown_fields(),
                rename_all: CaseRules::new(parsed.rename_all_rules()),
                rename_all_fields: CaseRules::new(parsed.rename_all_fields_rules()),
            }
        })
    }
}

/// Variant-level serde attributes.
#[derive(Debug, Default)]
pub(crate) struct SerdeVariant {
    /// The name the schema documents.
    schema_name: String,
    /// `#[serde(skip)]`: serde neither writes nor reads the variant.
    pub(crate) skipped: bool,
    /// `#[serde(untagged)]` on the variant: it is written without the enum's tag.
    pub(crate) untagged: bool,
    /// The enum's `deny_unknown_fields` applies to the fields of this variant.
    pub(crate) deny_unknown_fields: bool,
}

impl SerdeVariant {
    /// Read the attributes of the variant `ident` of `container`. Also returns the convention for
    /// the variant's fields: its own `rename_all`, else the container's `rename_all_fields`.
    pub(crate) fn parse(ident: &syn::Ident, attrs: &[syn::Attribute], container: &SerdeContainer) -> (Self, CaseRules) {
        with_context(|cx| {
            let variant = syn::Variant {
                attrs: attrs.to_vec(),
                ident: ident.clone(),
                fields: syn::Fields::Unit,
                discriminant: None,
            };
            let mut parsed = attr::Variant::from_ast(cx, &variant);
            parsed.rename_by_rules(container.rename_all.directions);
            let name = parsed.name();
            let common = container.rename_all.common.apply_to_variant(&ident.unraw().to_string());
            let serde = SerdeVariant {
                schema_name: schema_name(name.serialize_name(), name.deserialize_name(), common),
                skipped: parsed.skip_serializing() && parsed.skip_deserializing(),
                untagged: parsed.untagged(),
                deny_unknown_fields: container.closes_objects(),
            };
            (serde, CaseRules::new(parsed.rename_all_rules()).or(container.rename_all_fields))
        })
    }

    /// The name the schema documents for the variant.
    pub(crate) fn schema_name(&self) -> &str {
        &self.schema_name
    }
}

/// Field-level serde attributes.
#[derive(Debug, Default)]
pub(crate) struct SerdeField {
    /// The name the schema documents.
    schema_name: String,
    /// The name serde deserializes the field from.
    wire_name: String,
    /// The name serde serializes the field to.
    serialize_name: String,
    /// The names serde deserializes the field from: `wire_name` and every `alias`.
    read_names: BTreeSet<String>,
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
    /// Read the attributes of the field `ident` under the case convention `rules`.
    pub(crate) fn parse(ident: Option<&syn::Ident>, attrs: &[syn::Attribute], rules: CaseRules) -> Self {
        with_context(|cx| {
            // The unit type keeps serde from implying a `borrow` deserializer for `Cow` fields,
            // and a container default stops it from implying a field default for
            // `skip_deserializing`: only what the field states is recorded.
            let field = syn::Field {
                attrs: attrs.to_vec(),
                vis: syn::Visibility::Inherited,
                mutability: syn::FieldMutability::None,
                ident: ident.cloned(),
                colon_token: None,
                ty: syn::parse_quote!(()),
            };
            let mut parsed = attr::Field::from_ast(cx, 0, &field, None, &attr::Default::Default);
            parsed.rename_by_rules(rules.directions);
            let name = parsed.name();
            let common = ident.map_or_else(String::new, |ident| rules.common.apply_to_field(&ident.unraw().to_string()));
            SerdeField {
                schema_name: schema_name(name.serialize_name(), name.deserialize_name(), common),
                wire_name: name.deserialize_name().to_owned(),
                serialize_name: name.serialize_name().to_owned(),
                read_names: parsed.aliases().clone(),
                skip_serializing: parsed.skip_serializing(),
                skip_deserializing: parsed.skip_deserializing(),
                flatten: parsed.flatten(),
                default: !parsed.default().is_none(),
                deserialize_with: parsed.deserialize_with().is_some(),
                skip_serializing_if: parsed.skip_serializing_if().is_some(),
                skips_none: parsed.skip_serializing_if().is_some_and(is_option_is_none),
            }
        })
    }

    /// Whether serde drops the field in either direction (`skip`, `skip_serializing`,
    /// `skip_deserializing`).
    pub(crate) fn skipped(&self) -> bool {
        self.skip_serializing || self.skip_deserializing
    }

    /// Whether serde neither writes nor reads the field (`skip`, or both one-way skips).
    pub(crate) fn skipped_both_ways(&self) -> bool {
        self.skip_serializing && self.skip_deserializing
    }

    /// The name the schema documents for the field.
    pub(crate) fn schema_name(&self) -> &str {
        &self.schema_name
    }

    /// The name serde *deserializes* the field from. Validation errors are reported under it, as
    /// it is the key a client actually sent.
    pub(crate) fn wire_name(&self) -> &str {
        &self.wire_name
    }

    /// Whether serde writes and reads the field under different names, so the schema leaves one out.
    pub(crate) fn has_split_name(&self) -> bool {
        !self.skipped() && self.serialize_name != self.wire_name
    }

    /// Whether serde may write or read the field under `name`, through an `alias` too.
    pub(crate) fn is_named(&self, name: &str) -> bool {
        !self.skipped_both_ways() && (self.serialize_name == name || self.read_names.contains(name))
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

/// Whether a `skip_serializing_if` predicate, as serde parsed it, is recognisably `Option::is_none`.
///
/// Deliberately conservative: only the unqualified and the `std`/`core` paths, with optional
/// leading `::` and without generic arguments, are recognized. An alias or a local wrapper
/// predicate keeps the null branch.
fn is_option_is_none(predicate: &syn::ExprPath) -> bool {
    if predicate.qself.is_some() || predicate.path.segments.iter().any(|segment| !segment.arguments.is_none()) {
        return false;
    }
    let segments: Vec<String> = predicate.path.segments.iter().map(|segment| segment.ident.to_string()).collect();
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    matches!(
        segments.as_slice(),
        ["Option", "is_none"] | ["std", "option", "Option", "is_none"] | ["core", "option", "Option", "is_none"]
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    const NO_RULES: RenameAllRules = RenameAllRules {
        serialize: RenameRule::None,
        deserialize: RenameRule::None,
    };

    fn both(rule: RenameRule) -> RenameAllRules {
        RenameAllRules {
            serialize: rule,
            deserialize: rule,
        }
    }

    fn field(field: syn::Field) -> SerdeField {
        SerdeField::parse(field.ident.as_ref(), &field.attrs, CaseRules::new(NO_RULES))
    }

    fn fields_under(item: syn::DeriveInput, rules: RenameAllRules) -> Vec<SerdeField> {
        let syn::Data::Struct(data) = item.data else { unreachable!() };
        data.fields
            .iter()
            .map(|field| SerdeField::parse(field.ident.as_ref(), &field.attrs, CaseRules::new(rules)))
            .collect()
    }

    #[test]
    fn a_field_is_named_by_what_serde_writes_and_reads() {
        let item: syn::DeriveInput = syn::parse_quote! {
            struct Example {
                some_field: String,
                #[serde(rename(serialize = "out"))]
                written: String,
                #[serde(rename(deserialize = "in"))]
                read: String,
                #[serde(alias = "other")]
                aliased: String,
                #[serde(skip)]
                skipped: String,
            }
        };
        let fields = fields_under(item, both(RenameRule::CamelCase));

        assert!(fields[0].is_named("someField") && !fields[0].is_named("some_field"));
        assert!(fields[1].is_named("out") && fields[1].is_named("written") && !fields[1].is_named("in"));
        assert!(fields[2].is_named("in") && fields[2].is_named("read") && !fields[2].is_named("out"));
        assert!(fields[3].is_named("other") && fields[3].is_named("aliased"));
        assert!(!fields[4].is_named("skipped"));
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
        let fields = fields_under(item.clone(), NO_RULES);
        assert_eq!(fields[0].wire_name(), "wire-in");

        let fields = fields_under(item, both(RenameRule::UpperCase));
        // A plain `rename` applies to both directions.
        assert_eq!(fields[1].wire_name(), "both");
        // A serialize-only rename leaves deserialization on the Rust name, under the convention.
        assert_eq!(fields[2].wire_name(), "SERIALIZE_ONLY");
        // Renaming a field to its own name still opts it out of the case convention.
        assert_eq!(fields[3].wire_name(), "renamed_to_itself");
    }

    #[test]
    fn field_names_follow_serde_rather_than_splitting_words() {
        let item: syn::DeriveInput = syn::parse_quote! {
            struct Example {
                r#type: String,
                userID: String,
                _leading: String,
                trailing_: String,
                double__underscore: String,
                user_name: String,
            }
        };
        let names = |rule| {
            fields_under(item.clone(), both(rule))
                .iter()
                .map(|field| {
                    assert_eq!(field.schema_name(), field.wire_name());
                    field.schema_name().to_owned()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(RenameRule::None),
            ["type", "userID", "_leading", "trailing_", "double__underscore", "user_name"]
        );
        assert_eq!(
            names(RenameRule::SnakeCase),
            ["type", "userID", "_leading", "trailing_", "double__underscore", "user_name"]
        );
        assert_eq!(
            names(RenameRule::LowerCase),
            ["type", "userID", "_leading", "trailing_", "double__underscore", "user_name"]
        );
        assert_eq!(
            names(RenameRule::CamelCase),
            ["type", "userID", "leading", "trailing", "doubleUnderscore", "userName"]
        );
        assert_eq!(
            names(RenameRule::UpperCase),
            ["TYPE", "USERID", "_LEADING", "TRAILING_", "DOUBLE__UNDERSCORE", "USER_NAME"]
        );
        assert_eq!(
            names(RenameRule::KebabCase),
            ["type", "userID", "-leading", "trailing-", "double--underscore", "user-name"]
        );
    }

    #[test]
    fn variant_names_follow_serde_rather_than_splitting_words() {
        let container = |item: syn::DeriveInput| SerdeContainer::from_input(&item);
        let name = |container: &SerdeContainer, ident: syn::Ident| SerdeVariant::parse(&ident, &[], container).0.schema_name().to_owned();

        let snake = container(syn::parse_quote! { #[serde(rename_all = "snake_case")] enum E {} });
        assert_eq!(name(&snake, syn::parse_quote!(HTTPServer)), "h_t_t_p_server");
        assert_eq!(name(&snake, syn::parse_quote!(InProgress)), "in_progress");
        assert_eq!(name(&snake, syn::parse_quote!(r#Type)), "type");

        let camel = container(syn::parse_quote! { #[serde(rename_all = "camelCase")] enum E {} });
        assert_eq!(name(&camel, syn::parse_quote!(HTTPServer)), "hTTPServer");

        let plain = container(syn::parse_quote! { enum E {} });
        assert_eq!(name(&plain, syn::parse_quote!(r#Type)), "Type");
    }

    #[test]
    fn only_a_variant_skipped_in_both_directions_is_skipped() {
        let container = SerdeContainer::from_input(&syn::parse_quote! { enum E {} });
        let ident: syn::Ident = syn::parse_quote!(V);
        let skipped = |attr: syn::Attribute| SerdeVariant::parse(&ident, &[attr], &container).0.skipped;

        assert!(skipped(syn::parse_quote!(#[serde(skip)])));
        assert!(skipped(syn::parse_quote!(#[serde(skip_serializing, skip_deserializing)])));
        assert!(!skipped(syn::parse_quote!(#[serde(skip_serializing)])));
        assert!(!skipped(syn::parse_quote!(#[serde(skip_deserializing)])));
    }

    #[test]
    fn a_variants_own_convention_comes_before_the_containers() {
        let container = SerdeContainer::from_input(&syn::parse_quote! { #[serde(rename_all_fields = "camelCase")] enum E {} });
        let ident: syn::Ident = syn::parse_quote!(V);

        let (_, rules) = SerdeVariant::parse(&ident, &[], &container);
        assert!(rules.directions.serialize == RenameRule::CamelCase);

        let attrs: Vec<syn::Attribute> = vec![syn::parse_quote!(#[serde(rename_all = "kebab-case")])];
        let (_, rules) = SerdeVariant::parse(&ident, &attrs, &container);
        assert!(rules.directions.serialize == RenameRule::KebabCase);
    }

    #[test]
    fn a_schema_name_keeps_a_direction_it_already_documents() {
        let item: syn::DeriveInput = syn::parse_quote! {
            struct Example {
                #[serde(rename = "both")]
                single: String,
                #[serde(rename(serialize = "same", deserialize = "same"))]
                same: String,
                #[serde(rename(serialize = "out", deserialize = "in"))]
                split: String,
                #[serde(rename(serialize = "out"))]
                serialize_only: String,
                #[serde(rename(deserialize = "in"))]
                deserialize_only: String,
                #[serde(rename(serialize = "out", deserialize = "OWN_NAME"))]
                own_name: String,
            }
        };
        let names = |rules| {
            fields_under(item.clone(), rules)
                .iter()
                .map(|field| field.schema_name().to_owned())
                .collect::<Vec<_>>()
        };

        assert_eq!(names(NO_RULES), ["both", "same", "out", "serialize_only", "deserialize_only", "out"]);
        // Under a convention for both directions the identifier is what one direction uses.
        assert_eq!(
            names(both(RenameRule::UpperCase)),
            ["both", "same", "out", "SERIALIZE_ONLY", "DESERIALIZE_ONLY", "OWN_NAME"]
        );
    }

    #[test]
    fn a_convention_for_one_direction_keeps_the_identifier() {
        let item: syn::DeriveInput = syn::parse_quote! { struct Example { user_name: String, id: String } };
        let names = |rules| {
            fields_under(item.clone(), rules)
                .iter()
                .map(|field| field.schema_name().to_owned())
                .collect::<Vec<_>>()
        };
        let rules = |serialize, deserialize| RenameAllRules { serialize, deserialize };

        assert_eq!(names(rules(RenameRule::CamelCase, RenameRule::None)), ["user_name", "id"]);
        assert_eq!(names(rules(RenameRule::None, RenameRule::CamelCase)), ["user_name", "id"]);
        // Wrong in both directions before, so the serialize side applies.
        assert_eq!(names(rules(RenameRule::CamelCase, RenameRule::KebabCase)), ["userName", "id"]);
        assert_eq!(names(both(RenameRule::CamelCase)), ["userName", "id"]);
        // The identifier is what serde reads here, so it stays.
        assert_eq!(names(rules(RenameRule::CamelCase, RenameRule::SnakeCase)), ["user_name", "id"]);
    }

    #[test]
    fn variant_names_keep_a_direction_they_already_document() {
        let name = |container: syn::DeriveInput, attrs: Vec<syn::Attribute>| {
            let ident: syn::Ident = syn::parse_quote!(SomeVariant);
            SerdeVariant::parse(&ident, &attrs, &SerdeContainer::from_input(&container))
                .0
                .schema_name()
                .to_owned()
        };
        let plain = || -> syn::DeriveInput {
            syn::parse_quote! { enum E {} }
        };

        assert_eq!(name(plain(), vec![syn::parse_quote!(#[serde(rename(serialize = "out"))])]), "SomeVariant");
        assert_eq!(
            name(plain(), vec![syn::parse_quote!(#[serde(rename(serialize = "out", deserialize = "in"))])]),
            "out"
        );
        assert_eq!(
            name(syn::parse_quote! { #[serde(rename_all(serialize = "kebab-case"))] enum E {} }, vec![]),
            "SomeVariant"
        );
        assert_eq!(
            name(
                syn::parse_quote! { #[serde(rename_all(serialize = "kebab-case", deserialize = "snake_case"))] enum E {} },
                vec![]
            ),
            "some-variant"
        );
    }

    #[test]
    fn list_form_conventions_reach_the_fields_of_a_variant() {
        let container = SerdeContainer::from_input(&syn::parse_quote! {
            #[serde(rename_all_fields(serialize = "camelCase", deserialize = "kebab-case"))]
            enum E {}
        });
        let ident: syn::Ident = syn::parse_quote!(V);
        let (_, rules) = SerdeVariant::parse(&ident, &[], &container);
        assert!(rules.directions.serialize == RenameRule::CamelCase);
        assert!(rules.directions.deserialize == RenameRule::KebabCase);

        let attrs: Vec<syn::Attribute> = vec![syn::parse_quote!(#[serde(rename_all(serialize = "UPPERCASE", deserialize = "lowercase"))])];
        let (_, rules) = SerdeVariant::parse(&ident, &attrs, &container);
        assert!(rules.directions.serialize == RenameRule::UpperCase);
    }

    #[test]
    fn case_conventions_keep_both_directions() {
        let container = |item: syn::DeriveInput| SerdeContainer::from_input(&item).rename_all;

        let rules = container(syn::parse_quote! {
            #[serde(rename_all(deserialize = "camelCase"))]
            struct Example { value: String }
        });
        assert!(rules.directions.deserialize == RenameRule::CamelCase);
        assert!(rules.directions.serialize == RenameRule::None);
        assert!(rules.common == RenameRule::None);

        let rules = container(syn::parse_quote! {
            #[serde(rename_all = "kebab-case")]
            struct Example { value: String }
        });
        assert!(rules.common == RenameRule::KebabCase);

        let rules = container(syn::parse_quote! {
            #[serde(rename_all(serialize = "camelCase", deserialize = "camelCase"))]
            struct Example { value: String }
        });
        assert!(rules.common == RenameRule::CamelCase);

        let rules = container(syn::parse_quote! {
            #[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
            struct Example { value: String }
        });
        assert!(rules.directions.serialize == RenameRule::CamelCase);
        assert!(rules.directions.deserialize == RenameRule::KebabCase);
        assert!(rules.common == RenameRule::None);
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
    fn option_is_none_predicates_are_read_from_the_parsed_path() {
        for (predicate, recognized) in [
            ("Option::is_none", true),
            (" ::std::option::Option::is_none ", true),
            ("core::option::Option::is_none", true),
            ("Option ::is_none", true),
            ("Option :: is_none", true),
            ("std :: option :: Option :: is_none", true),
            ("Option::is_some", false),
            ("my::Option::is_none", false),
            ("Option::<u8>::is_none", false),
            ("is_none", false),
        ] {
            let attrs: Vec<syn::Attribute> = vec![syn::parse_quote!(#[serde(skip_serializing_if = #predicate)])];
            assert_eq!(SerdeField::parse(None, &attrs, CaseRules::new(NO_RULES)).skips_none, recognized, "{predicate}");
        }
    }

    #[test]
    fn a_proxy_documents_the_type_only_when_both_directions_convert_through_it() {
        let proxy = |item: syn::DeriveInput| SerdeContainer::from_input(&item).proxy;
        let ty = |ty: syn::Type| Some(ty);

        assert_eq!(proxy(syn::parse_quote! { struct S; }), None);
        assert_eq!(proxy(syn::parse_quote! { #[serde(from = "A")] struct S; }), None);
        assert_eq!(proxy(syn::parse_quote! { #[serde(try_from = "A")] struct S; }), None);
        assert_eq!(proxy(syn::parse_quote! { #[serde(into = "B")] struct S; }), None);
        assert_eq!(proxy(syn::parse_quote! { #[serde(from = "A", into = "B")] struct S; }), None);
        assert_eq!(proxy(syn::parse_quote! { #[serde(try_from = "A", into = "B")] struct S; }), None);
        assert_eq!(
            proxy(syn::parse_quote! { #[serde(from = "A", into = "A")] struct S; }),
            ty(syn::parse_quote!(A))
        );
        assert_eq!(
            proxy(syn::parse_quote! { #[serde(try_from = "A", into = "A")] struct S; }),
            ty(syn::parse_quote!(A))
        );
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
