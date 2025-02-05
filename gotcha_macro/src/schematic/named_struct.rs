use proc_macro2::TokenStream as TokenStream2;
use syn::{GenericParam};
use proc_macro2::Span;
use crate::schematic::ParameterStructFieldOpt;
use quote::quote;
use crate::utils::AttributesExt;

pub(crate) fn handler( ident: syn::Ident, doc: TokenStream2, fields: darling::ast::Fields<ParameterStructFieldOpt>) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();
    
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

                // handle properties
                let mut field_schema = <#field_ty as Schematic>::generate_schema();
                field_schema.description = #field_description;
                properties.insert(#field_name.to_string(), field_schema.to_value());

                // handle required
                if <#field_ty as Schematic>::required() {
                    required_fields.push(#field_name.to_string());
                }
            }
        })
        .collect();
    let ret = quote! {
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
            
            let mut required_fields = vec![];
            let mut properties = ::std::collections::HashMap::new();
            #(
                #fields_stream
            )*
            schema.extras.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).unwrap());
            schema.extras.insert("required".to_string(), ::gotcha::serde_json::to_value(required_fields).unwrap());
            schema
        }
    };

    Ok(ret)

}