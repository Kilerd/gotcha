use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::untagged_enum::WrappedValue;
use crate::schematic::ParameterEnumVariantOpt;

pub(crate) fn handler(core: &TokenStream2, ident_string: String, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>) -> syn::Result<TokenStream2> {
    // An `#[serde(untagged)]` branch has no tag and can accept what another branch accepts, and
    // serde takes the first match.
    let union_keyword = if variants.iter().any(|variant| variant.serde.untagged) {
        "anyOf"
    } else {
        "oneOf"
    };

    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            if variant.serde.untagged {
                let branch = variant.untagged_branch_object(core, WrappedValue::Referenced);
                return quote! { let variant_object = #branch; };
            }

            let varient_string = variant.serde_name();
            let variant_annotations = variant.variant_customizations(core);
            let style = variant.style();

            // A unit variant serializes as the bare string `"V"`, not as `{"V": {}}`.
            if matches!(style, darling::ast::Style::Unit) {
                return quote! {
                    let mut variant_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                    variant_object.insert("title".to_string(), #core::serde_json::to_value(#varient_string).expect("cannot convert title to value"));
                    variant_object.insert("type".to_string(), #core::serde_json::to_value("string").expect("cannot convert type to value"));
                    variant_object.insert("enum".to_string(), #core::serde_json::to_value(vec![#varient_string.to_string()]).expect("cannot convert values to value"));
                    #( #variant_annotations )*
                };
            }

            let content_expr: TokenStream2 = if matches!(style, darling::ast::Style::Tuple) && variant.fields.fields.len() == 1 {
                // A newtype variant `V(T)` serializes as `{"V": <T>}`: its content is `T`'s
                // schema, with the field's `#[schematic(..)]` annotations applied.
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
            } else if matches!(style, darling::ast::Style::Tuple) {
                // A tuple variant `V(A, B)` (including `V()`) serializes as `{"V": [a, b]}`.
                crate::schematic::tuple_variant_schema(core, &variant.fields.fields)
            } else {
                // Struct variant: the shared emitter applies skip/flatten/rename and
                // `#[schematic(..)]` to its fields.
                let content = crate::schematic::named_variant_object(core, None, variant.fields.fields, false);
                quote! {
                    #core::serde_json::to_value(#content).expect("cannot convert variant content to value")
                }
            };

            quote! {
                   let mut root_properties: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                   let mut root_required_fields: Vec<String> = vec![ #varient_string.to_string() ];
                   root_properties.insert(#varient_string.to_string(), #content_expr);

                   let mut variant_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                   variant_object.insert("title".to_string(), #core::serde_json::to_value(#varient_string).expect("cannot convert title to value"));
                   variant_object.insert("type".to_string(), #core::serde_json::to_value("object").expect("cannot convert type to value"));
                   variant_object.insert("properties".to_string(), #core::serde_json::to_value(root_properties).expect("cannot convert root properties to value"));
                   variant_object.insert("required".to_string(), #core::serde_json::to_value(root_required_fields).expect("cannot convert root properties required fields to value"));
                   #( #variant_annotations )*
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
                let flattening = false;

                #(
                    #variants_codegen
                    branches.push(variant_object);
                )*

                schema.schema.extras.insert(#union_keyword.to_string(), #core::serde_json::to_value(branches).unwrap());
                schema
            })
        }

        fn flatten_schema() -> Option<#core::serde_json::Value> {
            // Return the union schema for flattening
            let mut branches: Vec<::std::collections::HashMap<String, #core::serde_json::Value>> = vec![];
            let flattening = true;
            #(
                #variants_codegen
                branches.push(variant_object);
            )*
            let mut obj: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
            obj.insert(#union_keyword.to_string(), #core::serde_json::to_value(branches).unwrap());
            Some(#core::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
