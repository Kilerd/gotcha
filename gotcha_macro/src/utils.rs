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

#[cfg(test)]
mod tests {
    use super::*;

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
