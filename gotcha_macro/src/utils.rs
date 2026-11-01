use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{Attribute, Lit, Meta};

/// Resolve `package` to the path the calling crate uses for it, honouring Cargo dependency
/// renames. `FoundCrate::Itself` resolves to the package's own name; the crates in this
/// workspace make that valid with `extern crate self as <name>`.
pub(crate) fn crate_path(package: &str) -> Option<TokenStream2> {
    let name = match crate_name(package).ok()? {
        FoundCrate::Itself => package.to_owned(),
        FoundCrate::Name(name) => name,
    };
    let ident = syn::Ident::new(&name, Span::call_site());
    Some(quote! { ::#ident })
}

/// The `gotcha` crate as the calling crate names it, for attribute macros that emit framework
/// paths. Cargo dependency renames (`web = { package = "gotcha" }`) are resolved by package
/// name, so `#[api]`, `#[state]` and `#[config]` work under a renamed dependency.
///
/// When the caller's manifest cannot be inspected (a build system without a `Cargo.toml`, or a
/// package that only re-exports gotcha under the `gotcha` name), fall back to `::gotcha`, which
/// is what these macros emitted before renames were recognized.
pub(crate) fn gotcha_crate_path() -> TokenStream2 {
    crate_path("gotcha").unwrap_or_else(|| quote! { ::gotcha })
}

pub trait AttributesExt {
    fn get_doc(&self) -> Option<String>;
}

/// Serde rename_all case conventions
// Variant names intentionally mirror serde's case-convention names.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameAll {
    /// lowercase
    LowerCase,
    /// UPPERCASE
    UpperCase,
    /// PascalCase
    PascalCase,
    /// camelCase
    CamelCase,
    /// snake_case
    SnakeCase,
    /// SCREAMING_SNAKE_CASE
    ScreamingSnakeCase,
    /// kebab-case
    KebabCase,
    /// SCREAMING-KEBAB-CASE
    ScreamingKebabCase,
}

impl RenameAll {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "lowercase" => Some(RenameAll::LowerCase),
            "UPPERCASE" => Some(RenameAll::UpperCase),
            "PascalCase" => Some(RenameAll::PascalCase),
            "camelCase" => Some(RenameAll::CamelCase),
            "snake_case" => Some(RenameAll::SnakeCase),
            "SCREAMING_SNAKE_CASE" => Some(RenameAll::ScreamingSnakeCase),
            "kebab-case" => Some(RenameAll::KebabCase),
            "SCREAMING-KEBAB-CASE" => Some(RenameAll::ScreamingKebabCase),
            _ => None,
        }
    }

    /// Apply rename_all transformation to any identifier.
    /// First normalizes to words, then converts to target case.
    pub fn apply(&self, name: &str) -> String {
        let words = split_into_words(name);
        match self {
            RenameAll::LowerCase => words.join("").to_lowercase(),
            RenameAll::UpperCase => words.join("").to_uppercase(),
            RenameAll::PascalCase => words_to_pascal_case(&words),
            RenameAll::CamelCase => words_to_camel_case(&words),
            RenameAll::SnakeCase => words_to_snake_case(&words),
            RenameAll::ScreamingSnakeCase => words_to_snake_case(&words).to_uppercase(),
            RenameAll::KebabCase => words_to_kebab_case(&words),
            RenameAll::ScreamingKebabCase => words_to_kebab_case(&words).to_uppercase(),
        }
    }
}

/// Split an identifier into words.
/// Handles both PascalCase/camelCase and snake_case/kebab-case inputs.
fn split_into_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current_word = String::new();

    // First, split by underscores and hyphens
    for part in s.split(['_', '-']) {
        if part.is_empty() {
            continue;
        }

        // Then split camelCase/PascalCase within each part
        let mut prev_was_upper = false;
        for (i, c) in part.chars().enumerate() {
            if c.is_uppercase() {
                if !current_word.is_empty() {
                    // Check if this is start of new word or continuation of acronym
                    let next_is_lower = part.chars().nth(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
                    if !prev_was_upper || next_is_lower {
                        words.push(current_word.to_lowercase());
                        current_word = String::new();
                    }
                }
                current_word.push(c);
                prev_was_upper = true;
            } else {
                current_word.push(c);
                prev_was_upper = false;
            }
        }
        if !current_word.is_empty() {
            words.push(current_word.to_lowercase());
            current_word = String::new();
        }
    }

    words
}

fn words_to_pascal_case(words: &[String]) -> String {
    words
        .iter()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn words_to_camel_case(words: &[String]) -> String {
    let mut result = String::new();
    for (i, word) in words.iter().enumerate() {
        if i == 0 {
            result.push_str(&word.to_lowercase());
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_uppercase().next().unwrap());
                result.extend(chars);
            }
        }
    }
    result
}

fn words_to_snake_case(words: &[String]) -> String {
    words.join("_")
}

fn words_to_kebab_case(words: &[String]) -> String {
    words.join("-")
}

/// Parse serde rename attribute from field/variant attributes
pub fn parse_serde_rename(attrs: &[Attribute]) -> Option<String> {
    serde_name_value(attrs, "rename")
}

/// The name serde *deserializes* a field/variant from: `#[serde(rename = "..")]` applies to both
/// directions, while `#[serde(rename(deserialize = ".."))]` overrides only this one.
///
/// The schema generator intentionally keeps using [`parse_serde_rename`]; this reader exists so
/// validation errors can be reported under the key a client actually sent.
pub fn parse_serde_rename_deserialize(attrs: &[Attribute]) -> Option<String> {
    parse_serde_rename(attrs).or_else(|| serde_list_deserialize(attrs, "rename"))
}

/// The `deserialize = ".."` value inside a `#[serde(<key>(..))]` list, such as
/// `rename(serialize = "out", deserialize = "in")`.
fn serde_list_deserialize(attrs: &[Attribute], key: &str) -> Option<String> {
    attrs.iter().filter(|attr| attr.path.is_ident("serde")).find_map(|attr| {
        let nested = attr
            .parse_args_with(|input: syn::parse::ParseStream| syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input))
            .ok()?;
        nested.into_iter().find_map(|meta| match meta {
            syn::Meta::List(list) if list.path.is_ident(key) => list.nested.into_iter().find_map(|nested| match nested {
                syn::NestedMeta::Meta(syn::Meta::NameValue(name_value)) if name_value.path.is_ident("deserialize") => match name_value.lit {
                    syn::Lit::Str(lit_str) => Some(lit_str.value()),
                    _ => None,
                },
                _ => None,
            }),
            _ => None,
        })
    })
}

/// Check if field has serde(flatten) attribute
pub fn has_serde_flatten(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path.is_ident("serde") {
            if let Ok(nested) =
                attr.parse_args_with(|input: syn::parse::ParseStream| syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input))
            {
                for meta in nested {
                    if let syn::Meta::Path(path) = meta {
                        if path.is_ident("flatten") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Run `pred` over each item inside every `#[serde(...)]` attribute; return true if any matches.
fn serde_nested_any(attrs: &[Attribute], pred: impl Fn(&syn::Meta) -> bool) -> bool {
    for attr in attrs {
        if attr.path.is_ident("serde") {
            if let Ok(nested) =
                attr.parse_args_with(|input: syn::parse::ParseStream| syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input))
            {
                if nested.iter().any(&pred) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether a field is dropped by serde (`skip`, `skip_serializing`, `skip_deserializing`)
/// and should therefore not appear in the schema.
pub fn has_serde_skip(attrs: &[Attribute]) -> bool {
    serde_nested_any(
        attrs,
        |meta| matches!(meta, syn::Meta::Path(path) if path.is_ident("skip") || path.is_ident("skip_serializing") || path.is_ident("skip_deserializing")),
    )
}

/// Whether serde drops the field on *input* (`skip` or `skip_deserializing`). A field skipped
/// only for serialization is still read from the request, so it keeps a wire-name entry.
pub fn has_serde_skip_deserializing(attrs: &[Attribute]) -> bool {
    serde_nested_any(
        attrs,
        |meta| matches!(meta, syn::Meta::Path(path) if path.is_ident("skip") || path.is_ident("skip_deserializing")),
    )
}

/// Whether serde omits the field on *output* (`skip` or `skip_serializing`). A field skipped
/// only for deserialization is still serialized and stays in the schema.
pub fn has_serde_skip_serializing(attrs: &[Attribute]) -> bool {
    serde_nested_any(
        attrs,
        |meta| matches!(meta, syn::Meta::Path(path) if path.is_ident("skip") || path.is_ident("skip_serializing")),
    )
}

/// The case convention serde applies to *fields of struct variants*: the container-level
/// `#[serde(rename_all_fields = "..")]`. A variant's own `rename_all` wins over it.
pub fn parse_serde_rename_all_fields(attrs: &[Attribute]) -> Option<RenameAll> {
    serde_name_value(attrs, "rename_all_fields").and_then(|case| RenameAll::from_str(&case))
}

/// Whether serde supplies a missing value: `#[serde(default)]` or `#[serde(default = "...")]`.
pub fn has_serde_default(attrs: &[Attribute]) -> bool {
    serde_nested_any(attrs, |meta| match meta {
        syn::Meta::Path(path) => path.is_ident("default"),
        syn::Meta::NameValue(nv) => nv.path.is_ident("default"),
        _ => false,
    })
}

/// Whether deserialization uses a custom function or module — `#[serde(deserialize_with = "...")]`
/// or `#[serde(with = "...")]`. Without a `default`, serde then requires the key to be present.
pub fn has_serde_deserialize_with(attrs: &[Attribute]) -> bool {
    serde_nested_any(attrs, |meta| match meta {
        syn::Meta::NameValue(nv) => nv.path.is_ident("deserialize_with") || nv.path.is_ident("with"),
        _ => false,
    })
}

/// Whether the container is `#[serde(transparent)]`: it serializes as its single inner field.
pub fn has_serde_transparent(attrs: &[Attribute]) -> bool {
    serde_nested_any(attrs, |meta| matches!(meta, syn::Meta::Path(path) if path.is_ident("transparent")))
}

/// Whether rendering may omit the field (`#[serde(skip_serializing_if = "...")]`).
pub fn has_serde_skip_serializing_if(attrs: &[Attribute]) -> bool {
    serde_nested_any(
        attrs,
        |meta| matches!(meta, syn::Meta::NameValue(nv) if nv.path.is_ident("skip_serializing_if")),
    )
}

/// The string literal of a `#[serde(<key> = "...")]` attribute.
fn serde_name_value(attrs: &[Attribute], key: &str) -> Option<String> {
    attrs.iter().filter(|attr| attr.path.is_ident("serde")).find_map(|attr| {
        let nested = attr
            .parse_args_with(|input: syn::parse::ParseStream| syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input))
            .ok()?;
        nested.into_iter().find_map(|meta| match meta {
            syn::Meta::NameValue(nv) if nv.path.is_ident(key) => match nv.lit {
                syn::Lit::Str(lit) => Some(lit.value()),
                _ => None,
            },
            _ => None,
        })
    })
}

/// Whether `skip_serializing_if` is recognisably `Option::is_none`.
///
/// Deliberately conservative: only the unqualified and the `std`/`core` paths, with optional
/// leading `::`, are recognized. An alias or a local wrapper predicate keeps the null branch.
pub fn skips_none(attrs: &[Attribute]) -> bool {
    let Some(predicate) = serde_name_value(attrs, "skip_serializing_if") else {
        return false;
    };
    let predicate = predicate.trim();
    let predicate = predicate.strip_prefix("::").unwrap_or(predicate);
    matches!(predicate, "Option::is_none" | "std::option::Option::is_none" | "core::option::Option::is_none")
}

/// The `T` in an `Option<T>` type. Only a literal trailing `Option` path segment is recognized;
/// type aliases are left to their own `Schematic` implementation.
pub fn option_inner_type(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    if arguments.args.len() != 1 {
        return None;
    }
    match arguments.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// How serde attributes on a named field translate into schema presence.
pub(crate) struct SerdeFieldPresence {
    /// The type whose schema to emit. This is the inner `T` when an `Option<T>` field skips
    /// `None`: the serialized value is then `T`, so the outer null branch is not in the contract.
    pub schema_type: syn::Type,
    /// `Some` when the attributes override the type's own `Schematic::required()`.
    pub required: Option<bool>,
}

/// Translate the serde attributes already on a field into key presence and nullability.
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
pub(crate) fn serde_field_presence(ty: &syn::Type, attrs: &[Attribute], container_default: bool) -> SerdeFieldPresence {
    let has_default = has_serde_default(attrs) || container_default;
    let deserialize_with = has_serde_deserialize_with(attrs);
    let inner_option = option_inner_type(ty);
    let skips_none = inner_option.is_some() && skips_none(attrs);
    let asymmetric = deserialize_with && skips_none && !has_default;

    let required = if has_default || asymmetric {
        Some(false)
    } else if deserialize_with {
        Some(true)
    } else if has_serde_skip_serializing_if(attrs) {
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

/// Parse serde rename_all attribute from container attributes
pub fn parse_serde_rename_all(attrs: &[Attribute]) -> Option<RenameAll> {
    serde_name_value(attrs, "rename_all").and_then(|case| RenameAll::from_str(&case))
}

/// The case convention serde *deserializes* with: `#[serde(rename_all = "..")]` applies to both
/// directions, while `#[serde(rename_all(deserialize = ".."))]` overrides only this one.
///
/// The schema generator keeps using [`parse_serde_rename_all`]; this reader exists so validation
/// errors can be reported under the keys a client actually sent.
pub fn parse_serde_rename_all_deserialize(attrs: &[Attribute]) -> Option<RenameAll> {
    parse_serde_rename_all(attrs).or_else(|| serde_list_deserialize(attrs, "rename_all").and_then(|case| RenameAll::from_str(&case)))
}

/// Get the effective serialized name for a field/variant
pub fn get_serde_name(ident: &str, rename: Option<&str>, rename_all: Option<RenameAll>) -> String {
    if let Some(renamed) = rename {
        renamed.to_string()
    } else if let Some(case) = rename_all {
        case.apply(ident)
    } else {
        ident.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_into_words() {
        // PascalCase
        assert_eq!(split_into_words("InProgress"), vec!["in", "progress"]);
        assert_eq!(split_into_words("HTTPServer"), vec!["http", "server"]);
        assert_eq!(split_into_words("Simple"), vec!["simple"]);

        // snake_case
        assert_eq!(split_into_words("user_name"), vec!["user", "name"]);
        assert_eq!(split_into_words("created_at"), vec!["created", "at"]);

        // kebab-case
        assert_eq!(split_into_words("user-name"), vec!["user", "name"]);

        // Mixed
        assert_eq!(split_into_words("XMLParser"), vec!["xml", "parser"]);
    }

    #[test]
    fn test_rename_all_apply_pascal_case_input() {
        // Enum variants are typically PascalCase
        assert_eq!(RenameAll::KebabCase.apply("InProgress"), "in-progress");
        assert_eq!(RenameAll::SnakeCase.apply("InProgress"), "in_progress");
        assert_eq!(RenameAll::CamelCase.apply("InProgress"), "inProgress");
        assert_eq!(RenameAll::LowerCase.apply("InProgress"), "inprogress");
        assert_eq!(RenameAll::UpperCase.apply("InProgress"), "INPROGRESS");
        assert_eq!(RenameAll::ScreamingSnakeCase.apply("InProgress"), "IN_PROGRESS");
        assert_eq!(RenameAll::ScreamingKebabCase.apply("InProgress"), "IN-PROGRESS");
        assert_eq!(RenameAll::PascalCase.apply("InProgress"), "InProgress");
    }

    #[test]
    fn test_rename_all_apply_snake_case_input() {
        // Struct fields are typically snake_case
        assert_eq!(RenameAll::CamelCase.apply("user_name"), "userName");
        assert_eq!(RenameAll::PascalCase.apply("user_name"), "UserName");
        assert_eq!(RenameAll::KebabCase.apply("user_name"), "user-name");
        assert_eq!(RenameAll::SnakeCase.apply("user_name"), "user_name");
        assert_eq!(RenameAll::ScreamingSnakeCase.apply("max_retry_count"), "MAX_RETRY_COUNT");
    }

    #[test]
    fn test_get_serde_name() {
        // rename takes precedence
        assert_eq!(get_serde_name("InProgress", Some("custom"), Some(RenameAll::KebabCase)), "custom");
        // rename_all applied when no rename
        assert_eq!(get_serde_name("InProgress", None, Some(RenameAll::KebabCase)), "in-progress");
        // original name when neither
        assert_eq!(get_serde_name("InProgress", None, None), "InProgress");
    }

    #[test]
    fn deserialize_rename_reads_the_deserialize_direction() {
        let item: syn::DeriveInput = syn::parse_quote! {
            struct Example {
                #[serde(rename(serialize = "wire-out", deserialize = "wire-in"))]
                directed: String,
                #[serde(rename = "both")]
                both: String,
                #[serde(rename(serialize = "wire-out"))]
                serialize_only: String,
            }
        };
        let syn::Data::Struct(data) = &item.data else { unreachable!() };
        let attrs: Vec<&[syn::Attribute]> = data.fields.iter().map(|field| field.attrs.as_slice()).collect();

        assert_eq!(parse_serde_rename_deserialize(attrs[0]).as_deref(), Some("wire-in"));
        // A plain `rename` applies to both directions.
        assert_eq!(parse_serde_rename_deserialize(attrs[1]).as_deref(), Some("both"));
        // A serialize-only rename leaves deserialization on the Rust name.
        assert_eq!(parse_serde_rename_deserialize(attrs[2]), None);
    }

    #[test]
    fn doc_comments_skip_non_literal_attributes() {
        let attrs: Vec<Attribute> = vec![
            // `syn` cannot parse this argument list, but it is not a doc attribute.
            syn::parse_quote!(#[validate(regex(path = *NAME_RE))]),
            syn::parse_quote!(#[doc = "  field docs  "]),
            // A doc value that is an expression has no value at expansion time.
            syn::parse_quote!(#[doc = include_str!("../README.md")]),
            syn::parse_quote!(#[serde(rename_all = "camelCase")]),
        ];
        assert_eq!(attrs.get_doc().as_deref(), Some("field docs"));
        assert_eq!(Vec::<Attribute>::new().get_doc(), None);
    }

    #[test]
    fn deserialize_rename_all_reads_the_deserialize_direction() {
        let item: syn::DeriveInput = syn::parse_quote! {
            #[serde(rename_all(deserialize = "camelCase"))]
            struct Example {
                value: String,
            }
        };
        assert_eq!(parse_serde_rename_all_deserialize(&item.attrs), Some(RenameAll::CamelCase));

        let item: syn::DeriveInput = syn::parse_quote! {
            #[serde(rename_all = "kebab-case")]
            struct Example {
                value: String,
            }
        };
        assert_eq!(parse_serde_rename_all_deserialize(&item.attrs), Some(RenameAll::KebabCase));

        let item: syn::DeriveInput = syn::parse_quote! {
            #[serde(rename_all(serialize = "camelCase"))]
            struct Example {
                value: String,
            }
        };
        assert_eq!(parse_serde_rename_all_deserialize(&item.attrs), None);
    }

    #[test]
    fn the_attribute_macros_fall_back_to_the_conventional_crate_name() {
        // The macro crate itself has no `gotcha` dependency, which is the same situation as a
        // build system without a Cargo.toml or a facade that re-exports gotcha under its name.
        assert!(super::crate_path("gotcha").is_none());
        assert_eq!(super::gotcha_crate_path().to_string().replace(' ', ""), "::gotcha");
    }
}

impl AttributesExt for Vec<Attribute> {
    fn get_doc(&self) -> Option<String> {
        // Only `doc` attributes are read. Other attributes may contain arguments that `syn`'s
        // `Meta` cannot represent (`#[validate(regex(path = *RE))]`, a `length(max = CONST)`)
        // and parsing them here used to abort the whole macro expansion. A `doc` whose value is
        // not a string literal (`#[doc = include_str!(..)]`) has no value at macro-expansion
        // time and is skipped.
        let docs: Vec<String> = self
            .iter()
            .filter(|attr| attr.path.is_ident("doc"))
            .filter_map(|attr| match attr.parse_meta().ok()? {
                Meta::NameValue(doc) if doc.path.is_ident("doc") => match doc.lit {
                    Lit::Str(lit_str) => Some(lit_str.value()),
                    _ => None,
                },
                _ => None,
            })
            .map(|doc| doc.trim().to_string())
            .collect();
        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }
}
