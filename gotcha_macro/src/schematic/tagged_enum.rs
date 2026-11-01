use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::{ParameterEnumVariantOpt, VariantNaming};
use crate::utils::{get_serde_name, parse_serde_rename, parse_serde_rename_all};

/// Whether the field carries documentation worth copying onto the variant object.
fn has_annotations(description: &TokenStream2, customizations: &[TokenStream2]) -> bool {
    !customizations.is_empty() || description.to_string() != "None"
}

pub(crate) fn handler(
    core: &TokenStream2, ident_string: String, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>, naming: VariantNaming, tag_name: String,
) -> Result<TokenStream2, (Span, &'static str)> {
    let tag_name_str = tag_name.as_str();

    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let variant_ident_str = variant.ident.to_string();
            let variant_rename = parse_serde_rename(&variant.attrs);
            let varient_string = get_serde_name(&variant_ident_str, variant_rename.as_deref(), naming.variants);
            let field_rename_all = parse_serde_rename_all(&variant.attrs).or(naming.fields);

            let tag_property = quote! {
                let mut tag_enum = ::std::collections::HashMap::new();
                tag_enum.insert("type".to_string(), #core::serde_json::to_value("string").expect("cannot convert type to value"));
                tag_enum.insert("enum".to_string(), #core::serde_json::to_value(vec![#varient_string.to_string()]).expect("cannot convert values to value"));
                properties.insert(#tag_name_str.to_string(), #core::serde_json::to_value(tag_enum).expect("cannot convert tag to value"));
                properties_required_fields.push(#tag_name_str.to_string());
            };

            match variant.fields.style {
                darling::ast::Style::Struct => {
                    let object = crate::schematic::named_variant_object(core, Some((tag_name_str, &varient_string)), variant.fields.fields, field_rename_all);
                    quote! {
                        let mut variant_object = #object;
                        variant_object.insert("title".to_string(), #core::serde_json::to_value(#varient_string).expect("cannot convert title to value"));
                    }
                }
                darling::ast::Style::Unit => {
                    // Unit variant: only the tag.
                    quote! {
                        let mut properties: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                        let mut properties_required_fields: Vec<String> = vec![];
                        #tag_property
                        let mut variant_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                        variant_object.insert("title".to_string(), #core::serde_json::to_value(#varient_string).expect("cannot convert title to value"));
                        variant_object.insert("type".to_string(), #core::serde_json::to_value("object").expect("cannot convert type to value"));
                        variant_object.insert("properties".to_string(), #core::serde_json::to_value(properties).expect("cannot convert properties to value"));
                        variant_object.insert("required".to_string(), #core::serde_json::to_value(properties_required_fields).expect("cannot convert required fields to value"));
                    }
                }
                darling::ast::Style::Tuple => {
                    // Internally tagged newtype variant: the inner value's fields merge next to
                    // the tag. Only build a throwaway schema when the field carries annotations:
                    // calling `generate_schema` during collection would register components that
                    // the finished document never references.
                    let annotation_code: Vec<TokenStream2> = variant
                        .fields
                        .fields
                        .iter()
                        .filter_map(|field| {
                            let (description, customizations) = field.value_schema_customizations(core);
                            if !has_annotations(&description, &customizations) {
                                return None;
                            }
                            Some(quote! {
                                {
                                    let mut field_schema = #core::EnhancedSchema { schema: #core::oas::Schema::default(), required: true };
                                    field_schema.schema.description = #description;
                                    #( #customizations )*
                                    let annotations = field_schema.schema.to_value();
                                    if let Some(annotations) = annotations.as_object() {
                                        for key in ["description", "title"] {
                                            if let Some(value) = annotations.get(key) {
                                                variant_object.insert(key.to_string(), value.clone());
                                            }
                                        }
                                    }
                                }
                            })
                        })
                        .collect();
                    let field_merges: Vec<TokenStream2> = variant
                        .fields
                        .fields
                        .into_iter()
                        .map(|field| {
                            let field_ty = field.schema_type().clone();
                            quote! {
                                // A newtype variant is not a flatten field: serde still requires
                                // the fields of the value it wraps, including inside an `Option`.
                                for (inner_name, inner_schema) in <#field_ty as #core::Schematic>::fields() {
                                    properties.insert(inner_name.to_string(), inner_schema.schema.to_value());
                                    if inner_schema.required {
                                        properties_required_fields.push(inner_name.to_string());
                                    }
                                }
                                // A map-like inner value merges its entries next to the tag
                                // through `additionalProperties`; the trait method reports it
                                // without generating/registering the wrapper's own schema.
                                if collect_additional {
                                    if let Some(additional) = <#field_ty as #core::Schematic>::additional_properties() {
                                        inner_additional = Some(additional);
                                    }
                                }
                            }
                        })
                        .collect();
                    quote! {
                        let mut properties: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                        let mut properties_required_fields: Vec<String> = vec![];
                        let mut inner_additional: Option<#core::serde_json::Value> = None;
                        #tag_property
                        #( #field_merges )*
                        let mut variant_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                        if let Some(additional) = inner_additional {
                            variant_object.insert("additionalProperties".to_string(), additional);
                        }
                        variant_object.insert("title".to_string(), #core::serde_json::to_value(#varient_string).expect("cannot convert title to value"));
                        variant_object.insert("type".to_string(), #core::serde_json::to_value("object").expect("cannot convert type to value"));
                        variant_object.insert("properties".to_string(), #core::serde_json::to_value(properties).expect("cannot convert properties to value"));
                        variant_object.insert("required".to_string(), #core::serde_json::to_value(properties_required_fields).expect("cannot convert required fields to value"));
                        #( #annotation_code )*
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
                        format:None,
                        nullable:None,
                        description: Self::doc(),
                        extras: Default::default(),
                        ..#core::oas::Schema::default()
                    },
                    required: Self::required(),
                };
                let mut branches = vec![];
                // `flatten_schema` sets this to false: the map's value schema must not be
                // registered there when the branch constraint is stripped anyway.
                let collect_additional = true;

                #(
                    #variants_codegen
                    branches.push(variant_object);
                )*
                let mut discriminator = ::std::collections::HashMap::new();
                discriminator.insert("propertyName".to_string(), #core::serde_json::to_value(#tag_name_str).unwrap());
                schema.schema.extras.insert("oneOf".to_string(), #core::serde_json::to_value(branches).unwrap());
                schema.schema.extras.insert("discriminator".to_string(), #core::serde_json::to_value(discriminator).unwrap());
                schema
            })
        }

        fn flatten_schema() -> Option<#core::serde_json::Value> {
            // Return the oneOf schema for flattening. A branch's `additionalProperties` would
            // also constrain the fields the parent declares next to the flattened enum, so it is
            // dropped here (the map's entries stay undocumented at the parent level).
            let mut branches: Vec<::std::collections::HashMap<String, #core::serde_json::Value>> = vec![];
            let collect_additional = false;
            #(
                #variants_codegen
                branches.push(variant_object);
            )*
            for branch in &mut branches {
                branch.remove("additionalProperties");
                branch.remove("unevaluatedProperties");
            }
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
