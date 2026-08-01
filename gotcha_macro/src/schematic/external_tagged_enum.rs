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
            let varient_string = get_serde_name(&variant_ident_str, variant_rename.as_deref(), rename_all);

            // A newtype variant `V(T)` serializes as `{"V": <T>}`, so its content is exactly
            // `T`'s schema — including scalars. (Reconstructing from `T::fields()` produced an
            // empty object for scalar inners like `u32`.)
            let is_newtype = matches!(variant.fields.style, darling::ast::Style::Tuple) && variant.fields.fields.len() == 1;

            let content_expr: TokenStream2 = if is_newtype {
                let inner_ty = variant.fields.fields[0].ty.clone();
                quote! {
                    <#inner_ty as ::gotcha_core::Schematic>::generate_schema().schema.to_value()
                }
            } else {
                let fields_stream: Vec<TokenStream2> = variant
                    .fields
                    .fields
                    .into_iter()
                    .map(|field| {
                        let (field_description, customizations) = field.schema_customizations();
                        let field_ty = field.ty.clone();
                        if let Some(ident) = field.ident.as_ref() {
                            let field_ident_str = ident.to_string();
                            let field_rename = parse_serde_rename(&field.attrs);
                            let field_name = get_serde_name(&field_ident_str, field_rename.as_deref(), None);
                            quote! {
                                let mut field_schema = <#field_ty as ::gotcha_core::Schematic>::generate_schema();
                                field_schema.schema.description = #field_description;
                                #( #customizations )*
                                properties.insert(#field_name.to_string(), field_schema.schema.to_value());
                                if field_schema.required {
                                    properties_required_fields.push(#field_name.to_string());
                                }
                            }
                        } else {
                            quote! {
                                let varient_fields = <#field_ty as ::gotcha_core::Schematic>::fields();
                                for (inner_field_name, inner_field_schema) in varient_fields {
                                    properties.insert(inner_field_name.to_string(), inner_field_schema.schema.to_value());
                                    if inner_field_schema.required {
                                        properties_required_fields.push(inner_field_name.to_string());
                                    }
                                }
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
                        let mut second_properties: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
                        second_properties.insert("type".to_string(), ::gotcha_core::serde_json::to_value("object").expect("cannot convert type to value"));
                        second_properties.insert("properties".to_string(), ::gotcha_core::serde_json::to_value(properties).expect("cannot convert properties to value"));
                        second_properties.insert("required".to_string(), ::gotcha_core::serde_json::to_value(properties_required_fields).expect("cannot convert properties required fields to value"));
                        ::gotcha_core::serde_json::to_value(second_properties).expect("cannot convert second properties to value")
                    }
                }
            };

            quote! {
                   let mut root_properties: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
                   let mut root_required_fields: Vec<String> = vec![ #varient_string.to_string() ];
                   root_properties.insert(#varient_string.to_string(), #content_expr);

                   let mut variant_object: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
                   variant_object.insert("title".to_string(), ::gotcha_core::serde_json::to_value(#varient_string).expect("cannot convert title to value"));
                   variant_object.insert("type".to_string(), ::gotcha_core::serde_json::to_value("object").expect("cannot convert type to value"));
                   variant_object.insert("properties".to_string(), ::gotcha_core::serde_json::to_value(root_properties).expect("cannot convert root properties to value"));
                   variant_object.insert("required".to_string(), ::gotcha_core::serde_json::to_value(root_required_fields).expect("cannot convert root properties required fields to value"));
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
            ::gotcha_core::registry::schema_or_ref(Self::name(), Self::required(), || {
                let mut schema = ::gotcha_core::EnhancedSchema {
                    schema: ::gotcha_core::oas::Schema {
                        _type: None,
                        format:None,
                        nullable:None,
                        description: Self::doc(),
                        extras:Default::default()
                    },
                    required: Self::required(),
                };
                let mut branches = vec![];

                #(
                    #variants_codegen
                    branches.push(variant_object);
                )*

                schema.schema.extras.insert("oneOf".to_string(), ::gotcha_core::serde_json::to_value(branches).unwrap());
                schema
            })
        }

        fn flatten_schema() -> Option<::gotcha_core::serde_json::Value> {
            // Return the oneOf schema for flattening
            let mut branches: Vec<::std::collections::HashMap<String, ::gotcha_core::serde_json::Value>> = vec![];
            #(
                #variants_codegen
                branches.push(variant_object);
            )*
            let mut obj: ::std::collections::HashMap<String, ::gotcha_core::serde_json::Value> = ::std::collections::HashMap::new();
            obj.insert("oneOf".to_string(), ::gotcha_core::serde_json::to_value(branches).unwrap());
            Some(::gotcha_core::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
