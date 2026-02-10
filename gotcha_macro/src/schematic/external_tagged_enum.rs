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
            let variant_ident_str = variant.ident.to_string();
            let variant_rename = parse_serde_rename(&variant.attrs);
            let varient_string = get_serde_name(&variant_ident_str, variant_rename.as_deref(), rename_all);

            let fields_stream: Vec<TokenStream2> = variant
                .fields
                .into_iter()
                .map(|field| {
                    let field_ty = field.ty.clone();
                    let field_description = if let Some(doc) = field.attrs.get_doc() {
                        quote! { Some(#doc.to_string()) }
                    } else {
                        quote! {None}
                    };
                     if let Some(ident) = field.ident.as_ref() {
                        let field_ident_str = ident.to_string();
                        let field_rename = parse_serde_rename(&field.attrs);
                        // Note: for external tagged enum variant fields, serde uses the variant's rename_all if present
                        // For simplicity, we don't support nested rename_all on variants here
                        let field_name = get_serde_name(&field_ident_str, field_rename.as_deref(), None);
                        quote! {
                            let mut field_schema = <#field_ty as Schematic>::generate_schema();
                            field_schema.schema.description = #field_description;
                            properties.insert(#field_name.to_string(), field_schema.schema.to_value());

                            if field_schema.required {
                                properties_required_fields.push(#field_name.to_string());
                            }
                        }

                    } else {
                        quote! {
                            let mut varient_fields = <#field_ty as Schematic>::fields();

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
                   let mut properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                   let mut properties_required_fields: Vec<String> = vec![];
                   #(
                       #fields_stream
                   )*

                   let mut second_properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                   second_properties.insert("type".to_string(), ::gotcha::serde_json::to_value("object").expect("cannot convert type to value"));
                   second_properties.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).expect("cannot convert properties to value"));
                   second_properties.insert("required".to_string(), ::gotcha::serde_json::to_value(properties_required_fields).expect("cannot convert properties required fields to value"));

                   let mut root_properties: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                   let mut root_required_fields: Vec<String> = vec![ #varient_string.to_string() ];
                   root_properties.insert(#varient_string.to_string(), ::gotcha::serde_json::to_value(second_properties).expect("cannot convert properties to value"));

                   let mut variant_object: ::std::collections::HashMap<String, ::gotcha::serde_json::Value> = ::std::collections::HashMap::new();
                   variant_object.insert("type".to_string(), ::gotcha::serde_json::to_value("object").expect("cannot convert type to value"));
                   variant_object.insert("properties".to_string(), ::gotcha::serde_json::to_value(root_properties).expect("cannot convert root properties to value"));
                   variant_object.insert("required".to_string(), ::gotcha::serde_json::to_value(root_required_fields).expect("cannot convert root properties required fields to value"));


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

            schema.schema.extras.insert("oneOf".to_string(), ::gotcha::serde_json::to_value(branches).unwrap());
            schema
        }
    };

    Ok(ret)
}
