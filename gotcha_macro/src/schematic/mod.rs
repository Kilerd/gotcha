use darling::ast::Data;
use darling::{FromDeriveInput, FromField, FromVariant};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{parse2, DeriveInput, GenericParam};

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
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg, serde))]
pub(crate) struct ParameterOpts {
    ident: syn::Ident,
    generics: syn::Generics,
    where_clause: Option<syn::WhereClause>,
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
                quote! { ::gotcha_core::serde_json::Value::from(#v) }
            }
            syn::Lit::Int(v) => {
                let v = v.base10_parse::<i64>().map_err(darling::Error::from)?;
                quote! { ::gotcha_core::serde_json::Value::from(#v) }
            }
            syn::Lit::Float(v) => {
                let v = v.base10_parse::<f64>().map_err(darling::Error::from)?;
                quote! { ::gotcha_core::serde_json::Value::from(#v) }
            }
            syn::Lit::Bool(v) => {
                let v = v.value;
                quote! { ::gotcha_core::serde_json::Value::from(#v) }
            }
            _ => return Err(darling::Error::unexpected_lit_type(value)),
        };
        Ok(SchemaValue(tokens))
    }
}

#[derive(Debug, FromField)]
#[darling(attributes(schematic), forward_attrs(allow, doc, cfg, serde))]
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

impl ParameterStructFieldOpt {
    /// Builds the `description` token and the list of `#[schematic(...)]` customization
    /// statements for this field. Each statement mutates a local `field_schema` binding
    /// (its `.schema.format` / `.schema.extras`). An explicit `#[schematic(description = "...")]`
    /// overrides the doc comment.
    pub(crate) fn schema_customizations(&self) -> (TokenStream2, Vec<TokenStream2>) {
        // Builds a statement that inserts a JSON-Schema keyword into `field_schema.schema.extras`.
        fn extra(key: &str, value: TokenStream2) -> TokenStream2 {
            quote! {
                field_schema.schema.extras.insert(#key.to_string(), ::gotcha_core::serde_json::to_value(#value).unwrap());
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
        if let Some(format) = &self.format {
            customizations.push(quote! { field_schema.schema.format = Some(#format.to_string()); });
        }
        if let Some(v) = &self.title {
            customizations.push(extra("title", quote! { #v }));
        }
        // `example` / `default` keep their JSON type (`SchemaValue` already builds a `Value`).
        if let Some(v) = &self.example {
            let value = &v.0;
            customizations.push(quote! { field_schema.schema.extras.insert("example".to_string(), #value); });
        }
        if let Some(v) = &self.default {
            let value = &v.0;
            customizations.push(quote! { field_schema.schema.extras.insert("default".to_string(), #value); });
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

pub(crate) fn handler(input: TokenStream2) -> Result<TokenStream2, (Span, &'static str)> {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let param_opts: ParameterOpts = ParameterOpts::from_derive_input(&x1).unwrap();
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

    let generics_params = param_opts.generics.params.iter().map(|p| quote! { #p }).collect::<Vec<TokenStream2>>();
    let generics_single = param_opts
        .generics
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Type(ty) => {
                let ident = ty.ident.clone();
                quote! { #ident }
            }
            GenericParam::Lifetime(lt) => quote! { #lt },
            GenericParam::Const(c) => quote! { #c },
        })
        .collect::<Vec<TokenStream2>>();
    let generics = if generics_params.is_empty() {
        quote! {}
    } else {
        quote! {<#(#generics_params),*> }
    };
    let generics_single = if generics_single.is_empty() {
        quote! {}
    } else {
        quote! {<#(#generics_single),*> }
    };
    let where_clause = if let Some(where_clause) = param_opts.where_clause {
        quote! { where #where_clause }
    } else {
        quote! {}
    };

    let impl_stream = match param_opts.data {
        Data::Enum(enum_variants) => {
            // Check if all enum variants have empty fields
            let is_simple_enum = enum_variants.iter().all(|variant| variant.fields.is_empty());
            if is_simple_enum {
                simple_enum::handler(ident.clone(), doc, enum_variants, extra_field.rename_all)?
            } else {
                match extra_field.tag_kind {
                    None => {
                        // Default: externally tagged
                        external_tagged_enum::handler(ident.clone(), doc, enum_variants, extra_field.rename_all)?
                    }
                    Some(SerdeTagKind::Internal(ref tag_name)) => {
                        tagged_enum::handler(ident.clone(), doc, enum_variants, extra_field.rename_all, tag_name.clone())?
                    }
                    Some(SerdeTagKind::Adjacent { ref tag, ref content }) => {
                        adjacent_tagged_enum::handler(ident.clone(), doc, enum_variants, extra_field.rename_all, tag.clone(), content.clone())?
                    }
                    Some(SerdeTagKind::Untagged) => untagged_enum::handler(ident.clone(), doc, enum_variants, extra_field.rename_all)?,
                }
            }
        }
        Data::Struct(fields) => {
            let is_tuple = matches!(fields.style, darling::ast::Style::Tuple);
            let field_count = fields.fields.len();
            if is_tuple && field_count == 1 {
                // Newtype struct (e.g. `struct UserId(Uuid);`) — transparent to the inner type.
                newtype_struct::handler(fields.fields)
            } else if is_tuple {
                return Err((
                    ident.span(),
                    "#[derive(Schematic)] does not support multi-field tuple structs; use a named struct",
                ));
            } else {
                named_struct::handler(ident.clone(), doc, fields, extra_field.rename_all)?
            }
        }
    };

    let ret = quote! {
        impl #generics Schematic for #ident #generics_single #where_clause {
            #impl_stream
        }
    };

    Ok(ret)
}
