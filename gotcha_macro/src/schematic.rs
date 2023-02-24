use darling::{FromDeriveInput, FromField};

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{parse2, DeriveInput};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(parameter))]
struct ParameterOpts {
    ident: syn::Ident,
    // fall over the serde info
    data: darling::ast::Data<darling::util::Ignored, ParameterStructFieldOpt>,
}

#[derive(Debug, FromField)]
#[darling(attributes(parameter))]
struct ParameterStructFieldOpt {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    // for number
    maximum: Option<i64>,
    minimum: Option<i64>,
    // for string
    max_length: Option<u64>,
    min_length: Option<u64>,
    pattern: Option<String>,
    // array
    max_items: Option<u64>,
    min_items: Option<u64>,
}

pub(crate) fn handler(
    input: TokenStream2,
) -> Result<TokenStream2, (Span, &'static str)> {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let crud_opts: ParameterOpts = ParameterOpts::from_derive_input(&x1).unwrap();

    let ident = crud_opts.ident.clone();
    let ident_string = ident.to_string();

    // todo handle enum
    let fields = crud_opts.data.take_struct().unwrap();
    let fields_stream: Vec<TokenStream2> = fields
        .fields
        .iter()
        .map(|field| {

            let field_name = field.ident.unwrap().to_string();
            let field_ty = &field.ty;
            quote!{
                properties.insert(#field_name.to_string(), <#field_ty as Schematic>::generate_schema().to_value());
            }

        })
        .collect();
    let impl_stream = quote! {

        impl Schematic for #ident {
            fn name() -> &'static str {
                #ident_string
            }

            fn required() -> bool {
                true
            }

            fn type_() -> &'static str {
                "object"
            }
            fn generate_schema() -> Schema {
                let mut schema = Schema{
                    _type: Some(Self::type_().to_string()),
                    format:None,
                    nullable:None,
                    extras:Default::default()
                };
                let mut properties = ::std::collections::HashMap::new();
                #(
                    #fields_stream
                )*
                schema.extras.insert("properties".to_string(), ::serde_json::to_value(properties).unwrap());
                schema
            }
            fn is_match(&self) -> bool {
                true
            }
        }
    };
    Ok(impl_stream)
}



