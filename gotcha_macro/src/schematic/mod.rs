use darling::ast::Data;
use darling::{FromDeriveInput, FromField, FromVariant};
use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{parse2, DeriveInput};

pub mod adjacent_tagged_enum;
pub mod external_tagged_enum;
pub mod named_struct;
pub mod newtype_struct;
pub mod simple_enum;
pub mod tagged_enum;
pub mod untagged_enum;

use crate::utils::{parse_serde_rename_all, AttributesExt, RenameAll};

#[derive(Debug, PartialEq, Eq)]
enum SerdeTagKind {
    /// #[serde(tag = "type")] - internally tagged
    Internal(String),
    /// #[serde(tag = "type", content = "data")] - adjacently tagged
    Adjacent { tag: String, content: String },
    /// #[serde(untagged)] - no tag
    Untagged,
}

#[derive(Debug)]
struct ParameterExtraField {
    tag_kind: Option<SerdeTagKind>,
    rename_all: Option<RenameAll>,
}

impl ParameterExtraField {
    fn from_attr(attrs: &[syn::Attribute]) -> Self {
        let mut tag_name: Option<String> = None;
        let mut content_name: Option<String> = None;
        let mut is_untagged = false;
        let rename_all = parse_serde_rename_all(attrs);

        for attr in attrs {
            if attr.path.is_ident("serde") {
                if let Ok(nested) =
                    attr.parse_args_with(|input: syn::parse::ParseStream| syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input))
                {
                    for meta in nested {
                        match meta {
                            syn::Meta::NameValue(name_value) => {
                                if name_value.path.is_ident("tag") {
                                    if let syn::Lit::Str(lit_str) = name_value.lit {
                                        tag_name = Some(lit_str.value());
                                    }
                                } else if name_value.path.is_ident("content") {
                                    if let syn::Lit::Str(lit_str) = name_value.lit {
                                        content_name = Some(lit_str.value());
                                    }
                                }
                            }
                            syn::Meta::Path(path) if path.is_ident("untagged") => {
                                is_untagged = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let tag_kind = if is_untagged {
            Some(SerdeTagKind::Untagged)
        } else {
            match (tag_name, content_name) {
                (Some(tag), Some(content)) => Some(SerdeTagKind::Adjacent { tag, content }),
                (Some(tag), None) => Some(SerdeTagKind::Internal(tag)),
                _ => None,
            }
        };

        ParameterExtraField { tag_kind, rename_all }
    }
}

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(schematic), forward_attrs(allow, doc, cfg, serde))]
pub(crate) struct ParameterOpts {
    ident: syn::Ident,
    generics: syn::Generics,
    name: Option<String>,
    #[darling(rename = "crate")]
    crate_path: Option<syn::Path>,
    data: Data<ParameterEnumVariantOpt, ParameterStructFieldOpt>,
    attrs: Vec<syn::Attribute>,
}

/// A literal JSON value (`= 42`, `= "x"`, `= true`) whose JSON type is preserved. Used for
/// `example` / `default` so a numeric example stays a number instead of becoming a string.
#[derive(Debug, Clone)]
struct SchemaValue(TokenStream2);

impl darling::FromMeta for SchemaValue {
    fn from_value(value: &syn::Lit) -> darling::Result<Self> {
        let tokens = match value {
            syn::Lit::Str(v) => {
                let v = v.value();
                quote! { #v }
            }
            syn::Lit::Int(v) => {
                let v = v.base10_parse::<i64>().map_err(darling::Error::from)?;
                quote! { #v }
            }
            syn::Lit::Float(v) => {
                let v = v.base10_parse::<f64>().map_err(darling::Error::from)?;
                quote! { #v }
            }
            syn::Lit::Bool(v) => {
                let v = v.value;
                quote! { #v }
            }
            _ => return Err(darling::Error::unexpected_lit_type(value)),
        };
        Ok(SchemaValue(tokens))
    }
}

#[derive(Debug, FromField)]
#[darling(attributes(schematic), forward_attrs(allow, doc, cfg, serde, validate))]
pub(crate) struct ParameterStructFieldOpt {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    attrs: Vec<syn::Attribute>,

    // `#[schematic(...)]` field customizations — pure schema *documentation* only. darling
    // treats `Option<_>` fields as optional automatically, so an absent attribute maps to `None`.
    //
    // Validation constraints (min/max, length, pattern, multiple_of, items, …) are deliberately
    // NOT here: those belong to `#[validation(...)]` (issue #9) as the single source of truth for
    // both runtime request validation and the schema, so a constraint is never written twice.
    title: Option<String>,
    description: Option<String>,
    example: Option<SchemaValue>,
    default: Option<SchemaValue>,
    format: Option<String>,
}

/// The subset of `#[validate(...)]` (validator crate) rules that map cleanly to JSON-Schema
/// keywords. Unmodelled validators (`regex`, `custom`, `must_match`, `contains`, …) are ignored
/// so they stay runtime-only without breaking the derive.
#[derive(Default)]
struct ValidateConstraints {
    minimum: Option<f64>,
    maximum: Option<f64>,
    /// Whether the corresponding bound came from `exclusive_min` / `exclusive_max`.
    exclusive_minimum: bool,
    exclusive_maximum: bool,
    /// `length(min/max/equal)` — becomes minLength/maxLength for strings, minItems/maxItems for collections.
    min_length: Option<u64>,
    max_length: Option<u64>,
    format: Option<&'static str>,
}

impl ValidateConstraints {
    fn from_attrs(attrs: &[syn::Attribute]) -> Self {
        let mut c = ValidateConstraints::default();
        for attr in attrs {
            if !attr.path.is_ident("validate") {
                continue;
            }
            let items = match attr
                .parse_args_with(|input: syn::parse::ParseStream| syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input))
            {
                Ok(items) => items,
                Err(_) => continue,
            };
            for item in items {
                match item {
                    syn::Meta::List(list) if list.path.is_ident("range") => {
                        for (name, lit) in name_values(&list) {
                            match name.as_str() {
                                "min" => c.minimum = lit_to_f64(lit),
                                "max" => c.maximum = lit_to_f64(lit),
                                // JSON Schema stores exclusive bounds as numbers.
                                "exclusive_min" => {
                                    c.minimum = lit_to_f64(lit);
                                    c.exclusive_minimum = true;
                                }
                                "exclusive_max" => {
                                    c.maximum = lit_to_f64(lit);
                                    c.exclusive_maximum = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    syn::Meta::List(list) if list.path.is_ident("length") => {
                        for (name, lit) in name_values(&list) {
                            match name.as_str() {
                                "min" => c.min_length = lit_to_u64(lit),
                                "max" => c.max_length = lit_to_u64(lit),
                                // `equal` fixes both bounds.
                                "equal" => {
                                    c.min_length = lit_to_u64(lit);
                                    c.max_length = lit_to_u64(lit);
                                }
                                _ => {}
                            }
                        }
                    }
                    // `email` / `url` accept both the bare word and the `email(message = ...)` form.
                    syn::Meta::Path(p) if p.is_ident("email") => c.format = Some("email"),
                    syn::Meta::List(list) if list.path.is_ident("email") => c.format = Some("email"),
                    syn::Meta::Path(p) if p.is_ident("url") => c.format = Some("uri"),
                    syn::Meta::List(list) if list.path.is_ident("url") => c.format = Some("uri"),
                    _ => {}
                }
            }
        }
        c
    }
}

/// The `name = <lit>` pairs inside a `range(..)` / `length(..)` list.
fn name_values(list: &syn::MetaList) -> Vec<(String, &syn::Lit)> {
    list.nested
        .iter()
        .filter_map(|n| match n {
            syn::NestedMeta::Meta(syn::Meta::NameValue(nv)) => Some((nv.path.get_ident()?.to_string(), &nv.lit)),
            _ => None,
        })
        .collect()
}

fn lit_to_f64(lit: &syn::Lit) -> Option<f64> {
    match lit {
        syn::Lit::Int(i) => i.base10_parse().ok(),
        syn::Lit::Float(f) => f.base10_parse().ok(),
        _ => None,
    }
}

fn lit_to_u64(lit: &syn::Lit) -> Option<u64> {
    match lit {
        syn::Lit::Int(i) => i.base10_parse().ok(),
        _ => None,
    }
}

/// Whether `ty` is a collection (`Vec<_>`), peeling one layer of `Option<_>`. Used to decide
/// whether `length` maps to minItems/maxItems (collections) or minLength/maxLength (strings).
fn is_collection(ty: &syn::Type) -> bool {
    let syn::Type::Path(tp) = ty else {
        return false;
    };
    let Some(seg) = tp.path.segments.last() else {
        return false;
    };
    if seg.ident == "Vec" {
        return true;
    }
    if seg.ident != "Option" {
        return false;
    }
    // Peel one `Option<T>` layer and re-check the inner type.
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    match args.args.first() {
        Some(syn::GenericArgument::Type(inner)) => is_collection(inner),
        _ => false,
    }
}

impl ParameterStructFieldOpt {
    /// Builds the `description` token and the list of `#[schematic(...)]` customization
    /// statements for this field. Each statement mutates a local `field_schema` binding
    /// (its `.schema.format` / `.schema.extras`). An explicit `#[schematic(description = "...")]`
    /// overrides the doc comment.
    pub(crate) fn schema_customizations(&self, core: &TokenStream2) -> (TokenStream2, Vec<TokenStream2>) {
        // Builds a statement that inserts a JSON-Schema keyword into `field_schema.schema.extras`.
        fn extra(core: &TokenStream2, key: &str, value: TokenStream2) -> TokenStream2 {
            quote! {
                field_schema.schema.extras.insert(#key.to_string(), #core::serde_json::to_value(#value).unwrap());
            }
        }

        let description = if let Some(desc) = &self.description {
            quote! { Some(#desc.to_string()) }
        } else if let Some(doc) = self.attrs.get_doc() {
            quote! { Some(#doc.to_string()) }
        } else {
            quote! { None }
        };

        let mut customizations: Vec<TokenStream2> = Vec::new();

        // Mirror `#[validate(...)]` constraints into the schema so a rule written once for runtime
        // validation also documents the field. Emitted first, so an explicit `#[schematic(...)]`
        // still wins on any overlap (e.g. `format`).
        let validated = ValidateConstraints::from_attrs(&self.attrs);
        if let Some(min) = validated.minimum {
            let key = if validated.exclusive_minimum { "exclusiveMinimum" } else { "minimum" };
            customizations.push(extra(core, key, quote! { #min }));
        }
        if let Some(max) = validated.maximum {
            let key = if validated.exclusive_maximum { "exclusiveMaximum" } else { "maximum" };
            customizations.push(extra(core, key, quote! { #max }));
        }
        let (len_min, len_max) = if is_collection(&self.ty) {
            ("minItems", "maxItems")
        } else {
            ("minLength", "maxLength")
        };
        if let Some(min) = validated.min_length {
            customizations.push(extra(core, len_min, quote! { #min }));
        }
        if let Some(max) = validated.max_length {
            customizations.push(extra(core, len_max, quote! { #max }));
        }
        if let Some(format) = validated.format {
            customizations.push(quote! { field_schema.schema.format = Some(#format.to_string()); });
        }

        if let Some(format) = &self.format {
            customizations.push(quote! { field_schema.schema.format = Some(#format.to_string()); });
        }
        if let Some(v) = &self.title {
            customizations.push(extra(core, "title", quote! { #v }));
        }
        // `example` / `default` keep their JSON type when converted from the parsed literal.
        if let Some(v) = &self.example {
            let value = &v.0;
            customizations.push(quote! { field_schema.schema.extras.insert("examples".to_string(), #core::serde_json::json!([#value])); });
        }
        if let Some(v) = &self.default {
            let value = &v.0;
            customizations.push(quote! { field_schema.schema.extras.insert("default".to_string(), #core::serde_json::Value::from(#value)); });
        }

        (description, customizations)
    }
}

#[derive(Debug, FromVariant)]
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg, serde))]
pub(crate) struct ParameterEnumVariantOpt {
    ident: syn::Ident,
    #[allow(dead_code)]
    attrs: Vec<syn::Attribute>,
    fields: darling::ast::Fields<ParameterStructFieldOpt>,
}

// Resolve once per derive, then pass the same path through every schema generator.
// Libraries can stay on core alone; applications can use gotcha's re-export. Cargo
// dependency aliases are resolved by package name, including workspace dependencies.
fn core_crate_path() -> Result<TokenStream2, &'static str> {
    for package in ["gotcha_core", "gotcha"] {
        if let Ok(found) = crate_name(package) {
            let name = match found {
                FoundCrate::Itself => package.to_owned(),
                FoundCrate::Name(name) => name,
            };
            let ident = syn::Ident::new(&name, Span::call_site());
            return Ok(if package == "gotcha_core" {
                quote! { ::#ident }
            } else {
                quote! { ::#ident::gotcha_core }
            });
        }
    }
    Err("Schematic requires a gotcha_core or gotcha dependency; use #[schematic(crate = \"path::to::gotcha_core\")] for a custom re-export")
}

pub(crate) fn handler(input: TokenStream2) -> Result<TokenStream2, (Span, &'static str)> {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let param_opts: ParameterOpts = match ParameterOpts::from_derive_input(&x1) {
        Ok(opts) => opts,
        Err(error) => return Ok(error.write_errors()),
    };
    if let Some(name) = &param_opts.name {
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')) {
            return Err((
                param_opts.ident.span(),
                "schema name must be nonempty and contain only ASCII letters, digits, '.', '-' or '_'",
            ));
        }
    }
    let core = match &param_opts.crate_path {
        Some(path) => quote! { #path },
        None => core_crate_path().map_err(|message| (param_opts.ident.span(), message))?,
    };
    let schema_name = param_opts.name.clone().unwrap_or_else(|| param_opts.ident.to_string());
    let name_override = match &param_opts.name {
        Some(name) => quote! { Some(#name) },
        None => quote! { None },
    };
    let extra_field = ParameterExtraField::from_attr(&param_opts.attrs);
    let ident = param_opts.ident.clone();
    let doc = match param_opts.attrs.get_doc() {
        None => {
            quote! { None }
        }
        Some(t) => {
            quote! {Some( #t.to_owned()) }
        }
    };

    let (generics, generics_single, where_clause) = param_opts.generics.split_for_impl();

    let impl_stream = match param_opts.data {
        Data::Enum(enum_variants) => {
            // Check if all enum variants have empty fields
            let is_simple_enum = enum_variants.iter().all(|variant| variant.fields.is_empty());
            if is_simple_enum {
                simple_enum::handler(&core, schema_name.clone(), doc, enum_variants, extra_field.rename_all)?
            } else {
                match extra_field.tag_kind {
                    None => {
                        // Default: externally tagged
                        external_tagged_enum::handler(&core, schema_name.clone(), doc, enum_variants, extra_field.rename_all)?
                    }
                    Some(SerdeTagKind::Internal(ref tag_name)) => {
                        tagged_enum::handler(&core, schema_name.clone(), doc, enum_variants, extra_field.rename_all, tag_name.clone())?
                    }
                    Some(SerdeTagKind::Adjacent { ref tag, ref content }) => adjacent_tagged_enum::handler(
                        &core,
                        schema_name.clone(),
                        doc,
                        enum_variants,
                        extra_field.rename_all,
                        tag.clone(),
                        content.clone(),
                    )?,
                    Some(SerdeTagKind::Untagged) => untagged_enum::handler(&core, schema_name.clone(), doc, enum_variants, extra_field.rename_all)?,
                }
            }
        }
        Data::Struct(fields) => {
            let is_tuple = matches!(fields.style, darling::ast::Style::Tuple);
            let field_count = fields.fields.len();
            if is_tuple && field_count == 1 {
                // Newtype struct (e.g. `struct UserId(Uuid);`) — transparent to the inner type.
                newtype_struct::handler(&core, fields.fields, param_opts.name.as_deref())
            } else if is_tuple {
                return Err((
                    ident.span(),
                    "#[derive(Schematic)] does not support multi-field tuple structs; use a named struct",
                ));
            } else {
                named_struct::handler(&core, schema_name.clone(), doc, fields, extra_field.rename_all)?
            }
        }
    };

    let ret = quote! {
        impl #generics #core::Schematic for #ident #generics_single #where_clause {
            fn schema_name() -> Option<&'static str> { #name_override }
            #impl_stream
        }
    };

    Ok(ret)
}

#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn component_names_reject_invalid_openapi_keys() {
        for name in ["", "Bad/Name", "Bad~Name", "Bad Name", "Bad<Name>"] {
            let error = handler(quote! { #[schematic(name = #name)] struct Example { value: String } }).unwrap_err();
            assert!(error.1.contains("schema name must be nonempty"));
        }
        assert!(handler(quote! { #[schematic(crate = "::gotcha_core", name = "public.Result-v1")] struct Example { value: String } }).is_ok());
    }

    #[test]
    fn misspelled_container_options_report_compile_errors() {
        let output = handler(quote! { #[schematic(nmae = "Example")] struct Example { value: String } }).unwrap();
        assert!(output.to_string().contains("compile_error"));
    }
}
