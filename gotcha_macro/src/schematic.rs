use darling::{FromDeriveInput, FromField, FromVariant};
use darling::ast::Data;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{parse2, DeriveInput};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(parameter))]
struct ParameterOpts {
    ident: syn::Ident,
    // fall over the serde info
    data: darling::ast::Data<ParameterEnumVariantOpt, ParameterStructFieldOpt>,
}

#[derive(Debug, FromField)]
#[darling(attributes(serde, parameter))]
struct ParameterStructFieldOpt {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    // add more validator
}

#[derive(Debug, FromVariant)]
#[darling(attributes(serde, parameter))]
struct ParameterEnumVariantOpt {
    ident: syn::Ident,
}


pub(crate) fn handler(input: TokenStream2) -> Result<TokenStream2, (Span, &'static str)> {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let crud_opts: ParameterOpts = ParameterOpts::from_derive_input(&x1).unwrap();

    let ident = crud_opts.ident.clone();
    let ident_string = ident.to_string();


    let impl_stream = match crud_opts.data {
        Data::Enum(enum_variants) => {
            let variant_vec: Vec<TokenStream2> = enum_variants.into_iter()
                .map(|variant| variant.ident.to_string()).map(|variant_str| quote!{ #variant_str }).collect();
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
                    fn generate_schema() -> ::gotcha::oas::Schema {
                        let mut schema = ::gotcha::oas::Schema {
                            _type: Some(Self::type_().to_string()),
                            format:None,
                            nullable:None,
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
            let fields_stream: Vec<TokenStream2> = fields
                .fields
                .into_iter()
                .map(|field| {
                    let field_name = field.ident.unwrap().to_string();
                    let field_ty = field.ty;
                    quote! {
                properties.insert(#field_name.to_string(), <#field_ty as Schematic>::generate_schema().to_value());
            }
                })
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
                        "object"
                    }
                    fn generate_schema() -> ::gotcha::oas::Schema {
                        let mut schema = ::gotcha::oas::Schema {
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
                }
            }
        }
    };

    Ok(impl_stream)
}
