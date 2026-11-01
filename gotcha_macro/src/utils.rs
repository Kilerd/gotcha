use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{Attribute, Expr, ExprLit, Lit, Meta};

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

/// The literal `value` is, looking through the invisible groups a `macro_rules!` fragment such as
/// `$name:expr` arrives in (syn 1 read those as the literal itself).
pub(crate) fn expr_lit(value: &Expr) -> Option<&Lit> {
    match value {
        Expr::Lit(ExprLit { lit, .. }) => Some(lit),
        Expr::Group(group) => expr_lit(&group.expr),
        _ => None,
    }
}

/// The value of a string literal expression, as in `rename = "..."`.
pub(crate) fn lit_str(value: &Expr) -> Option<String> {
    match expr_lit(value)? {
        Lit::Str(lit) => Some(lit.value()),
        _ => None,
    }
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
            .filter_map(|attr| match &attr.meta {
                Meta::NameValue(doc) if doc.path.is_ident("doc") => lit_str(&doc.value),
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
