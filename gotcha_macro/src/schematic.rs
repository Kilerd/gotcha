use darling::ast::Data;
use darling::{FromDeriveInput, FromField, FromVariant};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{parse2, DeriveInput, GenericParam};

use crate::utils::AttributesExt;
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg))]
struct ParameterOpts {
    ident: syn::Ident,
    generics: syn::Generics,
    where_clause: Option<syn::WhereClause>,
    // fall over the serde info
    data: Data<ParameterEnumVariantOpt, ParameterStructFieldOpt>,
    attrs: Vec<syn::Attribute>,
}

#[derive(Debug, FromField)]
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg))]
struct ParameterStructFieldOpt {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    attrs: Vec<syn::Attribute>,
    // add more validator
}

#[derive(Debug, FromVariant)]
#[darling(attributes(parameter), forward_attrs(allow, doc, cfg))]
struct ParameterEnumVariantOpt {
    ident: syn::Ident,
    #[allow(dead_code)]
    attrs: Vec<syn::Attribute>,
}

pub(crate) fn handler(input: TokenStream2) -> Result<TokenStream2, (Span, &'static str)> {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let param_opts: ParameterOpts = ParameterOpts::from_derive_input(&x1).unwrap();

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
            let variant_vec: Vec<TokenStream2> = enum_variants
                .into_iter()
                .map(|variant| variant.ident.to_string())
                .map(|variant_str| quote! { #variant_str })
                .collect();
            quote! {
                impl Schematic for #ident {
                    fn name() -> &'static str {
                        #ident_string
                    }

                    fn required() -> bool {
                        true
                    }

                    fn type_() -> &'static str {
                        "string"
                    }
                    fn doc() -> Option<String> {
                        #doc
                    }
                    fn generate_schema() -> ::gotcha::oas::Schema {
                        let mut schema = ::gotcha::oas::Schema {
                            _type: Some(Self::type_().to_string()),
                            format:None,
                            nullable:None,
                            description: Self::doc(),
                            extras:Default::default()
                        };
                        let enum_variants:Vec<&'static str> = vec![ #(#variant_vec ,)* ];
                        schema.extras.insert("enum".to_string(), ::serde_json::to_value(enum_variants).unwrap());
                        schema
                    }
                }
            }
        }
        Data::Struct(fields) => {
            dbg!(&fields);
            dbg!(&param_opts.generics);

            let generics_params = param_opts.generics.params.iter().map(|p| quote! { #p }).collect::<Vec<TokenStream2>>();
            let generics_single =  param_opts.generics.params.iter().map(|p| {
                match p {
                    GenericParam::Type(ty) => {
                        let ident = ty.ident.clone();
                        quote! { #ident }
                    },
                    GenericParam::Lifetime(lt) => quote! { #lt },
                    GenericParam::Const(c) => quote! { #c },
                }
            }).collect::<Vec<TokenStream2>>();
            let generics = if generics_params.is_empty() {
                quote! { }
            } else {
                quote! {<#(#generics_params),*> }
            };
            let generics_single = if generics_single.is_empty() {
                quote! { }
            } else {
                quote! {<#(#generics_single),*> }
            };
            let where_clause = if let Some(where_clause) = param_opts.where_clause {
                quote! { where #where_clause }
            } else {
                quote! { }
            };
            let fields_stream: Vec<TokenStream2> = fields
                .fields
                .into_iter()
                .map(|field| {
                    let field_name = field.ident.unwrap().to_string();
                    let field_ty = field.ty;
                    let field_description = if let Some(doc) = field.attrs.get_doc() {
                        quote! { Some(#doc.to_string()) }
                    } else {
                        quote! {None}
                    };
                    quote! {
                        let mut field_schema = <#field_ty as Schematic>::generate_schema();
                        field_schema.description = #field_description;
                        properties.insert(#field_name.to_string(), field_schema.to_value());
                    }
                })
                .collect();
            let ret = quote! {
                impl #generics Schematic for #ident #generics_single {
                    fn name() -> &'static str {
                        #ident_string
                    }

                    fn required() -> bool {
                        true
                    }

                    fn type_() -> &'static str {
                        "object"
                    }
                    fn doc() -> Option<String> {
                        #doc
                    }
                    fn generate_schema() -> ::gotcha::oas::Schema {
                        let mut schema = ::gotcha::oas::Schema {
                            _type: Some(Self::type_().to_string()),
                            format:None,
                            nullable:None,
                            description: Self::doc(),
                            extras:Default::default()
                        };
                        let mut properties = ::std::collections::HashMap::new();
                        #(
                            #fields_stream
                        )*
                        schema.extras.insert("properties".to_string(), ::serde_json::to_value(properties).unwrap());
                        schema
                    }
                }
            };

            debug_print::debug_print!("token: {}", &ret);
            ret
        }
    };

    Ok(impl_stream)
}
