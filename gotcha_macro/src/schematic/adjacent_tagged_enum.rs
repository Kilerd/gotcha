use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;
use crate::utils::{get_serde_name, parse_serde_rename, RenameAll};

/// Handler for adjacently tagged enums: #[serde(tag = "kind", content = "data")]
/// JSON format: { "kind": "VariantName", "data": { ...variant fields... } }
pub(crate) fn handler(
    ident: syn::Ident,
    doc: TokenStream2,
    variants: Vec<ParameterEnumVariantOpt>,
    rename_all: Option<RenameAll>,
    tag_name: String,
    content_name: String,
) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();
    let tag_name_str = tag_name.as_str();
    let content_name_str = content_name.as_str();

    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let variant_ident_str = variant.ident.to_string();
            let variant_rename = parse_serde_rename(&variant.attrs);
            let variant_string = get_serde_name(&variant_ident_str, variant_rename.as_deref(), rename_all);

            let fields = variant.fields;
            let is_newtype = fields.len() == 1 && fields.fields[0].ident.is_none();

            if is_newtype {
                // Newtype variant: Variant(InnerType)
                // The content is the inner type's schema
                let inner_ty = &fields.fields[0].ty;
                quote! {
                    {
                        // Tag enum schema
                        let mut tag_enum: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        tag_enum.insert("type".to_string(), ::gotcha::serde_json::to_value("string").unwrap());
                        tag_enum.insert("enum".to_string(), ::gotcha::serde_json::to_value(vec![#variant_string]).unwrap());

                        // Content is the inner type's schema
                        let content_schema = <#inner_ty as Schematic>::generate_schema().schema.to_value();

                        // Build variant object
                        let mut properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        properties.insert(#tag_name_str.to_string(), ::gotcha::serde_json::to_value(tag_enum).unwrap());
                        properties.insert(#content_name_str.to_string(), content_schema);

                        let mut variant_object: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        variant_object.insert("type".to_string(), ::gotcha::serde_json::to_value("object").unwrap());
                        variant_object.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).unwrap());
                        variant_object.insert("required".to_string(), ::gotcha::serde_json::to_value(vec![#tag_name_str, #content_name_str]).unwrap());
                        variant_object
                    }
                }
            } else if fields.is_empty() {
                // Unit variant: Variant
                // Only has the tag, no content
                quote! {
                    {
                        // Tag enum schema
                        let mut tag_enum: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        tag_enum.insert("type".to_string(), ::gotcha::serde_json::to_value("string").unwrap());
                        tag_enum.insert("enum".to_string(), ::gotcha::serde_json::to_value(vec![#variant_string]).unwrap());

                        let mut properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        properties.insert(#tag_name_str.to_string(), ::gotcha::serde_json::to_value(tag_enum).unwrap());

                        let mut variant_object: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        variant_object.insert("type".to_string(), ::gotcha::serde_json::to_value("object").unwrap());
                        variant_object.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).unwrap());
                        variant_object.insert("required".to_string(), ::gotcha::serde_json::to_value(vec![#tag_name_str]).unwrap());
                        variant_object
                    }
                }
            } else {
                // Named fields variant: Variant { field1, field2, ... }
                // Content is an object with the fields
                let fields_stream: Vec<TokenStream2> = fields
                    .into_iter()
                    .filter_map(|field| {
                        let field_ident = field.ident.as_ref()?;
                        let field_ident_str = field_ident.to_string();
                        let field_rename = parse_serde_rename(&field.attrs);
                        let field_name = get_serde_name(&field_ident_str, field_rename.as_deref(), None);
                        let field_ty = &field.ty;
                        Some(quote! {
                            {
                                let field_schema = <#field_ty as Schematic>::generate_schema();
                                content_properties.insert(#field_name.to_string(), field_schema.schema.to_value());
                                if field_schema.required {
                                    content_required.push(#field_name.to_string());
                                }
                            }
                        })
                    })
                    .collect();

                quote! {
                    {
                        // Tag enum schema
                        let mut tag_enum: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        tag_enum.insert("type".to_string(), ::gotcha::serde_json::to_value("string").unwrap());
                        tag_enum.insert("enum".to_string(), ::gotcha::serde_json::to_value(vec![#variant_string]).unwrap());

                        // Build content object schema
                        let mut content_properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        let mut content_required: Vec<String> = vec![];
                        #(
                            #fields_stream
                        )*

                        let mut content_schema: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        content_schema.insert("type".to_string(), ::gotcha::serde_json::to_value("object").unwrap());
                        content_schema.insert("properties".to_string(), ::gotcha::serde_json::to_value(content_properties).unwrap());
                        content_schema.insert("required".to_string(), ::gotcha::serde_json::to_value(content_required).unwrap());

                        // Build variant object
                        let mut properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        properties.insert(#tag_name_str.to_string(), ::gotcha::serde_json::to_value(tag_enum).unwrap());
                        properties.insert(#content_name_str.to_string(), ::gotcha::serde_json::to_value(content_schema).unwrap());

                        let mut variant_object: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                        variant_object.insert("type".to_string(), ::gotcha::serde_json::to_value("object").unwrap());
                        variant_object.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).unwrap());
                        variant_object.insert("required".to_string(), ::gotcha::serde_json::to_value(vec![#tag_name_str, #content_name_str]).unwrap());
                        variant_object
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

            let branches: Vec<::std::collections::HashMap<String, ::gotcha::serde_json::Value>> = vec![
                #(
                    #variants_codegen,
                )*
            ];

            let mut discriminator: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
            discriminator.insert("propertyName".to_string(), ::gotcha::serde_json::to_value(#tag_name_str).unwrap());

            schema.schema.extras.insert("oneOf".to_string(), ::gotcha::serde_json::to_value(branches).unwrap());
            schema.schema.extras.insert("discriminator".to_string(), ::gotcha::serde_json::to_value(discriminator).unwrap());
            schema
        }

        fn flatten_schema() -> Option<::gotcha::serde_json::Value> {
            let branches: Vec<::std::collections::HashMap<String, ::gotcha::serde_json::Value>> = vec![
                #(
                    #variants_codegen,
                )*
            ];

            let mut discriminator: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
            discriminator.insert("propertyName".to_string(), ::gotcha::serde_json::to_value(#tag_name_str).unwrap());

            let mut obj: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
            obj.insert("oneOf".to_string(), ::gotcha::serde_json::to_value(branches).unwrap());
            obj.insert("discriminator".to_string(), ::gotcha::serde_json::to_value(discriminator).unwrap());
            Some(::gotcha::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
