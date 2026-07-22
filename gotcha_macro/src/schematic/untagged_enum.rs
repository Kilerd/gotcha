use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;
use crate::utils::{get_serde_name, parse_serde_rename, RenameAll};

pub(crate) fn handler(
    ident: syn::Ident, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>, rename_all: Option<RenameAll>,
) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();

    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let variant_ident_str = variant.ident.to_string();
            let variant_rename = parse_serde_rename(&variant.attrs);
            let variant_string = get_serde_name(&variant_ident_str, variant_rename.as_deref(), rename_all);

            let is_tuple = matches!(variant.fields.style, darling::ast::Style::Tuple);
            let field_count = variant.fields.fields.len();

            if is_tuple && field_count == 1 {
                // Single unnamed field (newtype): the inner type's schema directly.
                let inner_ty = variant.fields.fields[0].ty.clone();
                quote! {
                    <#inner_ty as ::gotcha_core::Schematic>::generate_schema().schema.to_value()
                }
            } else if is_tuple {
                // Multi-field tuple variant `V(A, B)` serializes as `[a, b]`. Emit an array that
                // preserves every element's schema (previously only the last field survived).
                let field_schemas: Vec<TokenStream2> = variant
                    .fields
                    .fields
                    .iter()
                    .map(|field| {
                        let field_ty = field.ty.clone();
                        quote! { <#field_ty as ::gotcha_core::Schematic>::generate_schema().schema.to_value() }
                    })
                    .collect();
                quote! {
                    {
                        let prefix_items: Vec<::gotcha_core::serde_json::Value> = vec![ #( #field_schemas ),* ];
                        let item_count = prefix_items.len();
                        let mut array_schema: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
                        array_schema.insert("type".to_string(), ::gotcha_core::serde_json::to_value("array").expect("cannot convert type to value"));
                        array_schema.insert("prefixItems".to_string(), ::gotcha_core::serde_json::to_value(prefix_items).expect("cannot convert prefixItems to value"));
                        array_schema.insert("minItems".to_string(), ::gotcha_core::serde_json::to_value(item_count).expect("cannot convert minItems to value"));
                        array_schema.insert("maxItems".to_string(), ::gotcha_core::serde_json::to_value(item_count).expect("cannot convert maxItems to value"));
                        ::gotcha_core::serde_json::to_value(array_schema).expect("cannot convert array schema to value")
                    }
                }
            } else {
                // Named (struct) variant: an object schema built from its fields.
                let fields_stream: Vec<TokenStream2> = variant
                    .fields
                    .fields
                    .into_iter()
                    .map(|field| {
                        let (field_description, customizations) = field.schema_customizations();
                        let field_ty = field.ty.clone();
                        let field_ident_str = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                        let field_rename = parse_serde_rename(&field.attrs);
                        let field_name = get_serde_name(&field_ident_str, field_rename.as_deref(), rename_all);
                        quote! {
                            let mut field_schema = <#field_ty as ::gotcha_core::Schematic>::generate_schema();
                            field_schema.schema.description = #field_description;
                            #( #customizations )*
                            properties.insert(#field_name.to_string(), field_schema.schema.to_value());
                            if field_schema.required {
                                properties_required_fields.push(#field_name.to_string());
                            }
                        }
                    })
                    .collect();

                quote! {
                    {
                        let mut properties: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
                        let mut properties_required_fields: Vec<String> = vec![];
                        #(
                            #fields_stream
                        )*
                        let mut variant_object: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
                        variant_object.insert("title".to_string(), ::gotcha_core::serde_json::to_value(#variant_string).expect("cannot convert title to value"));
                        variant_object.insert("type".to_string(), ::gotcha_core::serde_json::to_value("object").expect("cannot convert type to value"));
                        variant_object.insert("properties".to_string(), ::gotcha_core::serde_json::to_value(properties).expect("cannot convert properties to value"));
                        variant_object.insert("required".to_string(), ::gotcha_core::serde_json::to_value(properties_required_fields).expect("cannot convert required fields to value"));
                        ::gotcha_core::serde_json::to_value(variant_object).expect("cannot convert variant to value")
                    }
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
            "union"
        }

        fn doc() -> Option<String> {
            #doc
        }

        fn generate_schema() -> ::gotcha_core::EnhancedSchema {
            let mut schema = ::gotcha_core::EnhancedSchema {
                schema: ::gotcha_core::oas::Schema {
                    _type: None,
                    format: None,
                    nullable: None,
                    description: Self::doc(),
                    extras: Default::default(),
                },
                required: Self::required(),
            };

            let branches: Vec<::gotcha_core::serde_json::Value> = vec![
                #(
                    #variants_codegen,
                )*
            ];

            // untagged enum: oneOf without discriminator
            schema.schema.extras.insert("oneOf".to_string(), ::gotcha_core::serde_json::to_value(branches).unwrap());
            schema
        }

        fn flatten_schema() -> Option<::gotcha_core::serde_json::Value> {
            // Return the oneOf schema for flattening
            let branches: Vec<::gotcha_core::serde_json::Value> = vec![
                #(
                    #variants_codegen,
                )*
            ];
            let mut obj: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
            obj.insert("oneOf".to_string(), ::gotcha_core::serde_json::to_value(branches).unwrap());
            Some(::gotcha_core::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
