use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;
use crate::utils::{get_serde_name, parse_serde_rename, AttributesExt, RenameAll};

pub(crate) fn handler(
    ident: syn::Ident,
    doc: TokenStream2,
    variants: Vec<ParameterEnumVariantOpt>,
    rename_all: Option<RenameAll>,
) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();

    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let fields_stream: Vec<TokenStream2> = variant
                .fields
                .into_iter()
                .map(|field| {
                    let field_description = if let Some(doc) = field.attrs.get_doc() {
                        quote! { Some(#doc.to_string()) }
                    } else {
                        quote! { None }
                    };
                    let field_ty = field.ty.clone();

                    if let Some(ident) = field.ident.as_ref() {
                        // named variant
                        let field_ident_str = ident.to_string();
                        let field_rename = parse_serde_rename(&field.attrs);
                        let field_name = get_serde_name(&field_ident_str, field_rename.as_deref(), rename_all);
                        quote! {
                            let mut field_schema = <#field_ty as Schematic>::generate_schema();
                            field_schema.schema.description = #field_description;
                            properties.insert(#field_name.to_string(), field_schema.schema.to_value());
                            if field_schema.required {
                                properties_required_fields.push(#field_name.to_string());
                            }
                        }
                    } else {
                        // unnamed variant (tuple variant with single field)
                        // For untagged enum, this is typically the inner type's schema
                        quote! {
                            let inner_schema = <#field_ty as Schematic>::generate_schema();
                            // For single unnamed field, use the inner schema directly
                            is_single_unnamed = true;
                            single_unnamed_schema = Some(inner_schema.schema.to_value());
                        }
                    }
                })
                .collect();

            quote! {
                {
                    let mut properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                    let mut properties_required_fields: Vec<String> = vec![];
                    let mut is_single_unnamed = false;
                    let mut single_unnamed_schema: Option<::gotcha::serde_json::Value> = None;

                    #(
                        #fields_stream
                    )*

                    if is_single_unnamed {
                        // For unnamed variant, use the inner schema directly
                        single_unnamed_schema.unwrap()
                    } else {
                        // For named variant, create an object schema
                        let mut variant_object: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        variant_object.insert("type".to_string(), ::gotcha::serde_json::to_value("object").expect("cannot convert type to value"));
                        variant_object.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).expect("cannot convert properties to value"));
                        variant_object.insert("required".to_string(), ::gotcha::serde_json::to_value(properties_required_fields).expect("cannot convert required fields to value"));
                        ::gotcha::serde_json::to_value(variant_object).expect("cannot convert variant to value")
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

        fn generate_schema() -> ::gotcha::EnhancedSchema {
            let mut schema = ::gotcha::EnhancedSchema {
                schema: ::gotcha::oas::Schema {
                    _type: None,
                    format: None,
                    nullable: None,
                    description: Self::doc(),
                    extras: Default::default(),
                },
                required: Self::required(),
            };

            let branches: Vec<::gotcha::serde_json::Value> = vec![
                #(
                    #variants_codegen,
                )*
            ];

            // untagged enum: oneOf without discriminator
            schema.schema.extras.insert("oneOf".to_string(), ::gotcha::serde_json::to_value(branches).unwrap());
            schema
        }

        fn flatten_schema() -> Option<::gotcha::serde_json::Value> {
            // Return the oneOf schema for flattening
            let branches: Vec<::gotcha::serde_json::Value> = vec![
                #(
                    #variants_codegen,
                )*
            ];
            let mut obj: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
            obj.insert("oneOf".to_string(), ::gotcha::serde_json::to_value(branches).unwrap());
            Some(::gotcha::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
