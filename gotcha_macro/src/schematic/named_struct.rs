use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterStructFieldOpt;
use crate::utils::{get_serde_name, parse_serde_rename, AttributesExt, RenameAll};

pub(crate) fn handler(
    ident: syn::Ident,
    doc: TokenStream2,
    fields: darling::ast::Fields<ParameterStructFieldOpt>,
    rename_all: Option<RenameAll>,
) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();

    let fields_stream: Vec<TokenStream2> = fields
        .fields
        .into_iter()
        .map(|field| {
            let ident_str = field.ident.as_ref().unwrap().to_string();
            let rename = parse_serde_rename(&field.attrs);
            let field_name = get_serde_name(&ident_str, rename.as_deref(), rename_all);
            let field_ty = field.ty;
            let field_description = if let Some(doc) = field.attrs.get_doc() {
                quote! { Some(#doc.to_string()) }
            } else {
                quote! {None}
            };
            quote! {
                (
                    #field_name,

                    // handle properties
                    {
                        let mut field_schema = <#field_ty as Schematic>::generate_schema();
                        field_schema.schema.description = #field_description;
                        field_schema
                    }
                )
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
        fn fields() -> Vec<(&'static str, ::gotcha::EnhancedSchema)> {
            vec![
                #(
                    #fields_stream ,
                )*
            ]
        }
        fn generate_schema() -> ::gotcha::EnhancedSchema {
            let mut schema = ::gotcha::EnhancedSchema {
                schema: ::gotcha::oas::Schema {
                    _type: Some(Self::type_().to_string()),
                    format:None,
                    nullable:Self::nullable(),
                    description: Self::doc(),
                    extras:Default::default()
                },
                required: Self::required(),
            };

            let mut required_fields = vec![];
            let mut properties = ::std::collections::HashMap::new();

            let fields = Self::fields();
            for (field_name, field_schema) in fields {
                properties.insert(field_name.to_string(), field_schema.schema.to_value());
                if field_schema.required {
                    required_fields.push(field_name.to_string());
                }
            }
            schema.schema.extras.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).unwrap());
            schema.schema.extras.insert("required".to_string(), ::gotcha::serde_json::to_value(required_fields).unwrap());
            schema
        }
    };

    Ok(ret)
}
