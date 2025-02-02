use proc_macro2::TokenStream as TokenStream2;
use syn::{GenericParam};
use proc_macro2::Span;
use crate::schematic::ParameterStructFieldOpt;
use quote::quote;
use crate::utils::AttributesExt;

pub fn handler( ident: syn::Ident, doc: TokenStream2, generics: syn::Generics, where_clause: Option<syn::WhereClause>, fields: darling::ast::Fields<ParameterStructFieldOpt>) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();
    let generics_params = generics.params.iter().map(|p| quote! { #p }).collect::<Vec<TokenStream2>>();
    let generics_single =  generics.params.iter().map(|p| {
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
    let where_clause = if let Some(where_clause) = where_clause {
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

    Ok(ret)

}