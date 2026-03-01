use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterStructFieldOpt;
use crate::utils::{get_serde_name, has_serde_flatten, parse_serde_rename, AttributesExt, RenameAll};

pub(crate) fn handler(
    ident: syn::Ident,
    doc: TokenStream2,
    fields: darling::ast::Fields<ParameterStructFieldOpt>,
    rename_all: Option<RenameAll>,
) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();

    let mut normal_fields_stream: Vec<TokenStream2> = Vec::new();
    let mut flatten_fields_stream: Vec<TokenStream2> = Vec::new();
    let mut flatten_schema_stream: Vec<TokenStream2> = Vec::new();

    for field in fields.fields.into_iter() {
        let field_ty = field.ty;

        if has_serde_flatten(&field.attrs) {
            // Flatten field: merge fields from the inner type at runtime
            flatten_fields_stream.push(quote! {
                result.extend(<#field_ty as Schematic>::fields());
            });
            // Also collect flatten schemas for allOf generation
            flatten_schema_stream.push(quote! {
                if let Some(flatten_val) = <#field_ty as Schematic>::flatten_schema() {
                    flatten_schemas.push(flatten_val);
                }
            });
        } else {
            // Normal field: static field entry
            let ident_str = field.ident.as_ref().unwrap().to_string();
            let rename = parse_serde_rename(&field.attrs);
            let field_name = get_serde_name(&ident_str, rename.as_deref(), rename_all);
            let field_description = if let Some(doc) = field.attrs.get_doc() {
                quote! { Some(#doc.to_string()) }
            } else {
                quote! { None }
            };
            normal_fields_stream.push(quote! {
                (
                    #field_name,
                    {
                        let mut field_schema = <#field_ty as Schematic>::generate_schema();
                        field_schema.schema.description = #field_description;
                        field_schema
                    }
                )
            });
        }
    }

    let has_flatten = !flatten_schema_stream.is_empty();

    let generate_schema_impl = if has_flatten {
        // When there are flatten fields, we need to check at runtime if any
        // flatten field returns a non-None flatten_schema (i.e., is an enum)
        quote! {
            // Collect flatten schemas from enum types
            let mut flatten_schemas: Vec<::gotcha::serde_json::Value> = vec![];
            #(
                #flatten_schema_stream
            )*

            // Build the base object schema with own properties
            let mut required_fields = vec![];
            let mut properties = ::std::collections::HashMap::new();

            let fields = Self::fields();
            for (field_name, field_schema) in fields {
                properties.insert(field_name.to_string(), field_schema.schema.to_value());
                if field_schema.required {
                    required_fields.push(field_name.to_string());
                }
            }

            if flatten_schemas.is_empty() {
                // No enum flatten fields, just use simple object schema
                schema.schema._type = Some(Self::type_().to_string());
                schema.schema.extras.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).unwrap());
                schema.schema.extras.insert("required".to_string(), ::gotcha::serde_json::to_value(required_fields).unwrap());
            } else {
                // Has enum flatten fields, use allOf to combine
                let mut base_schema = ::std::collections::HashMap::new();
                base_schema.insert("type".to_string(), ::gotcha::serde_json::to_value("object").unwrap());
                base_schema.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).unwrap());
                base_schema.insert("required".to_string(), ::gotcha::serde_json::to_value(required_fields).unwrap());

                let mut all_of: Vec<::gotcha::serde_json::Value> = vec![
                    ::gotcha::serde_json::to_value(base_schema).unwrap()
                ];
                all_of.extend(flatten_schemas);

                schema.schema._type = None;
                schema.schema.extras.insert("allOf".to_string(), ::gotcha::serde_json::to_value(all_of).unwrap());
            }
            schema
        }
    } else {
        // No flatten fields, use simple implementation
        quote! {
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

    // Generate flatten_schema implementation for structs
    let flatten_schema_impl = if has_flatten {
        quote! {
            fn flatten_schema() -> Option<::gotcha::serde_json::Value> {
                // Collect flatten schemas from inner types (for nested flatten)
                let mut flatten_schemas: Vec<::gotcha::serde_json::Value> = vec![];
                #(
                    #flatten_schema_stream
                )*

                if flatten_schemas.is_empty() {
                    None
                } else if flatten_schemas.len() == 1 {
                    Some(flatten_schemas.remove(0))
                } else {
                    // Multiple enum flatten schemas - combine with allOf
                    let mut obj: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                    obj.insert("allOf".to_string(), ::gotcha::serde_json::to_value(flatten_schemas).unwrap());
                    Some(::gotcha::serde_json::to_value(obj).unwrap())
                }
            }
        }
    } else {
        quote! {}
    };

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
            let mut result: Vec<(&'static str, ::gotcha::EnhancedSchema)> = vec![
                #(
                    #normal_fields_stream ,
                )*
            ];
            #(
                #flatten_fields_stream
            )*
            result
        }

        #flatten_schema_impl

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

            #generate_schema_impl
        }
    };

    Ok(ret)
}
