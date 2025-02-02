use darling::ast::Data;
use darling::{FromDeriveInput, FromField, FromVariant};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{parse2, DeriveInput, GenericParam};
use darling::FromMeta;


pub mod named_struct;
pub mod simple_enum;
pub mod external_tagged_enum;
pub mod tagged_enum;

use crate::utils::AttributesExt;

#[derive(Debug, PartialEq, Eq)]
enum SerdeTag{
    Type,
}

impl  SerdeTag {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "type" => Some(SerdeTag::Type),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct ParameterExtraField {
    tag : Option<SerdeTag>,
}

impl ParameterExtraField {
    fn from_attr(attrs: &[syn::Attribute]) -> Self {
        let mut tag = None;

        for attr in attrs {
            if attr.path.is_ident("serde") {
                if let Ok(nested) = attr.parse_args_with(|input: syn::parse::ParseStream| {
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input)
                }) {
                    for meta in nested {
                        if let syn::Meta::NameValue(name_value) = meta {
                            if name_value.path.is_ident("tag") {
                                if let syn::Lit::Str(lit_str) = name_value.lit {
                                    tag = SerdeTag::from_str(&lit_str.value());
                                }
                            }
                        }
                    }
                }
            }
        }
        ParameterExtraField { tag }
    }
}

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg, serde))]
struct ParameterOpts {
    ident: syn::Ident,
    generics: syn::Generics,
    where_clause: Option<syn::WhereClause>,
    data: Data<ParameterEnumVariantOpt, ParameterStructFieldOpt>,
    attrs: Vec<syn::Attribute>,
}


#[derive(Debug, FromField)]
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg, serde))]
struct ParameterStructFieldOpt {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    attrs: Vec<syn::Attribute>,
    // add more validator
}

#[derive(Debug, FromVariant)]
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg, serde))]
struct ParameterEnumVariantOpt {
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
    let ident_string = ident.to_string();
    let doc = match param_opts.attrs.get_doc() {
        None => {
            quote! { None }
        }
        Some(t) => {
            quote! {Some( #t.to_owned()) }
        }
    };

    let impl_stream = match param_opts.data {
        Data::Enum(enum_variants) => {
            // Check if all enum variants have empty fields
            let is_simple_enum = enum_variants.iter().all(|variant| variant.fields.is_empty());
            if is_simple_enum {
                simple_enum::handler(ident, doc, enum_variants)?
            }
        else {
            if extra_field.tag ==None {
                external_tagged_enum::handler(ident, doc, enum_variants)?
            }
            else if extra_field.tag == Some(SerdeTag::Type) {
                tagged_enum::handler(ident, doc, enum_variants)?
            } else {
                return Err((
                    Span::call_site(),
                    "Only simple enums without fields are supported"
                ))
            }
        }},
        Data::Struct(fields) => {
            named_struct::handler(ident, doc, param_opts.generics, param_opts.where_clause, fields)?
        }
    };
    println!("{}", &impl_stream);
    Ok(impl_stream)
}
