use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;
use crate::utils::AttributesExt;

pub(crate) fn handler(ident: syn::Ident, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();

    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let varient_string = variant.ident.to_string();

            let fields_stream: Vec<TokenStream2> = variant
                .fields
                .into_iter()
                .map(|field| {
                    // doc code gen
                    let field_description = if let Some(doc) = field.attrs.get_doc() {
                        quote! { Some(#doc.to_string()) }
                    } else {
                        quote! {None}
                    };
                    let field_ty = field.ty;

                    if let Some(ident) = field.ident.as_ref() {
                        // named variant
                        let field_name = ident.to_string();
                        quote! {
                            let mut field_schema = <#field_ty as Schematic>::generate_schema();
                            field_schema.schema.description = #field_description;
                            properties.insert(#field_name.to_string(), field_schema.schema.to_value());
                            if field_schema.required {
                                properties_required_fields.push(#field_name.to_string());
                            }
                        }
                    } else {
                        // unnamed variant
                        quote! {
                            let inner_fields = <#field_ty as Schematic>::fields();
                            for (field_name, field_schema) in inner_fields {
                                properties.insert(field_name.to_string(), field_schema.schema.to_value());
                                if field_schema.required {
                                    properties_required_fields.push(field_name.to_string());
                                }
                            }
                        }
                    }
                })
                .collect();

            quote! {
                let mut single_enum = ::std::collections::HashMap::new();
                single_enum.insert("type".to_string(), ::gotcha::serde_json::to_value("enum").expect("cannot convert type to value"));
                single_enum.insert("values".to_string(), ::gotcha::serde_json::to_value(vec![#varient_string.to_string()]).expect("cannot convert values to value"));

                let mut properties = ::std::collections::HashMap::new();
                let mut properties_required_fields = vec![];
                properties.insert("type".to_string(), ::gotcha::serde_json::to_value(single_enum).expect("cannot convert type to value"));
                properties_required_fields.push("type".to_string());
                #(
                    #fields_stream
                )*

                let mut variant_object = ::std::collections::HashMap::new();
                variant_object.insert("type".to_string(), ::gotcha::serde_json::to_value("object").expect("cannot convert type to value"));
                variant_object.insert("properties".to_string(), ::gotcha::serde_json::to_value(properties).expect("cannot convert root properties to value"));
                variant_object.insert("required".to_string(), ::gotcha::serde_json::to_value(properties_required_fields).expect("cannot convert root required fields to value"));
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
            let mut discriminator = ::std::collections::HashMap::new();
            discriminator.insert("propertyName".to_string(), ::gotcha::serde_json::to_value("type").unwrap());
            schema.schema.extras.insert("oneOf".to_string(), ::gotcha::serde_json::to_value(branches).unwrap());
            schema.schema.extras.insert("discriminator".to_string(), ::gotcha::serde_json::to_value(discriminator).unwrap());
            schema
        }
    };

    Ok(ret)
}
