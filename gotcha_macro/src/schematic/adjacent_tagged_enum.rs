use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;

/// Handler for adjacently tagged enums: #[serde(tag = "kind", content = "data")]
/// JSON format: { "kind": "VariantName", "data": { ...variant fields... } }
pub(crate) fn handler(
    core: &TokenStream2, ident_string: String, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>, tag_name: String, content_name: String,
) -> syn::Result<TokenStream2> {
    let tag_name_str = tag_name.as_str();
    let content_name_str = content_name.as_str();

    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let variant_string = variant.serde_name();
            let variant_annotations = variant.variant_customizations(core);

            let tag_enum = quote! {
                let mut tag_enum: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                tag_enum.insert("type".to_string(), #core::serde_json::to_value("string").unwrap());
                tag_enum.insert("enum".to_string(), #core::serde_json::to_value(vec![#variant_string]).unwrap());
            };

            match variant.fields.style {
                darling::ast::Style::Struct => {
                    let content = crate::schematic::named_variant_object(core, None, variant.fields.fields);
                    quote! {
                        {
                            #tag_enum
                            let content_schema = #core::serde_json::to_value(#content).expect("cannot convert content to value");
                            let mut properties: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                            properties.insert(#tag_name_str.to_string(), #core::serde_json::to_value(tag_enum).unwrap());
                            properties.insert(#content_name_str.to_string(), content_schema);
                            let mut variant_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                            variant_object.insert("title".to_string(), #core::serde_json::to_value(#variant_string).unwrap());
                            variant_object.insert("type".to_string(), #core::serde_json::to_value("object").unwrap());
                            variant_object.insert("properties".to_string(), #core::serde_json::to_value(properties).unwrap());
                            variant_object.insert("required".to_string(), #core::serde_json::to_value(vec![#tag_name_str, #content_name_str]).unwrap());
                            #( #variant_annotations )*
                            variant_object
                        }
                    }
                }
                darling::ast::Style::Unit => {
                    // Unit variant: only the tag, no content.
                    quote! {
                        {
                            #tag_enum
                            let mut properties: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                            properties.insert(#tag_name_str.to_string(), #core::serde_json::to_value(tag_enum).unwrap());
                            let mut variant_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                            variant_object.insert("title".to_string(), #core::serde_json::to_value(#variant_string).unwrap());
                            variant_object.insert("type".to_string(), #core::serde_json::to_value("object").unwrap());
                            variant_object.insert("properties".to_string(), #core::serde_json::to_value(properties).unwrap());
                            variant_object.insert("required".to_string(), #core::serde_json::to_value(vec![#tag_name_str]).unwrap());
                            #( #variant_annotations )*
                            variant_object
                        }
                    }
                }
                darling::ast::Style::Tuple => {
                    let content_schema = if variant.fields.fields.len() == 1 {
                        let field = &variant.fields.fields[0];
                        let inner_ty = field.schema_type().clone();
                        let (description, customizations) = field.value_schema_customizations(core);
                        quote! {
                            {
                                let mut field_schema = <#inner_ty as #core::Schematic>::generate_schema();
                                let description: Option<String> = #description;
                                if let Some(description) = description {
                                    field_schema.schema.description = Some(description);
                                }
                                #( #customizations )*
                                field_schema.schema.to_value()
                            }
                        }
                    } else {
                        crate::schematic::tuple_variant_schema(core, &variant.fields.fields)
                    };
                    quote! {
                        {
                            #tag_enum
                            let content_schema = #content_schema;
                            let mut properties: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                            properties.insert(#tag_name_str.to_string(), #core::serde_json::to_value(tag_enum).unwrap());
                            properties.insert(#content_name_str.to_string(), content_schema);
                            let mut variant_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                            variant_object.insert("title".to_string(), #core::serde_json::to_value(#variant_string).unwrap());
                            variant_object.insert("type".to_string(), #core::serde_json::to_value("object").unwrap());
                            variant_object.insert("properties".to_string(), #core::serde_json::to_value(properties).unwrap());
                            variant_object.insert("required".to_string(), #core::serde_json::to_value(vec![#tag_name_str, #content_name_str]).unwrap());
                            #( #variant_annotations )*
                            variant_object
                        }
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

        fn generate_schema() -> #core::EnhancedSchema {
            #core::registry::schema_or_ref_for::<Self>(Self::schema_name(), module_path!(), Self::required(), || {
                let mut schema = #core::EnhancedSchema {
                    schema: #core::oas::Schema {
                        _type: None,
                        format: None,
                        nullable: None,
                        description: Self::doc(),
                        extras: Default::default(),
                        ..#core::oas::Schema::default()
                    },
                    required: Self::required(),
                };

                let branches: Vec<::std::collections::HashMap<String, #core::serde_json::Value>> = vec![
                    #(
                        #variants_codegen,
                    )*
                ];

                let mut discriminator: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                discriminator.insert("propertyName".to_string(), #core::serde_json::to_value(#tag_name_str).unwrap());

                schema.schema.extras.insert("oneOf".to_string(), #core::serde_json::to_value(branches).unwrap());
                schema.schema.extras.insert("discriminator".to_string(), #core::serde_json::to_value(discriminator).unwrap());
                schema
            })
        }

        fn flatten_schema() -> Option<#core::serde_json::Value> {
            let branches: Vec<::std::collections::HashMap<String, #core::serde_json::Value>> = vec![
                #(
                    #variants_codegen,
                )*
            ];

            let mut discriminator: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
            discriminator.insert("propertyName".to_string(), #core::serde_json::to_value(#tag_name_str).unwrap());

            let mut obj: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
            obj.insert("oneOf".to_string(), #core::serde_json::to_value(branches).unwrap());
            obj.insert("discriminator".to_string(), #core::serde_json::to_value(discriminator).unwrap());
            Some(#core::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
