use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::{ParameterEnumVariantOpt, VariantNaming};
use crate::utils::{get_serde_name, parse_serde_rename, parse_serde_rename_all};

pub(crate) fn handler(
    core: &TokenStream2, ident_string: String, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>, naming: VariantNaming,
) -> Result<TokenStream2, (Span, &'static str)> {
    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let variant_ident_str = variant.ident.to_string();
            let variant_rename = parse_serde_rename(&variant.attrs);
            let variant_string = get_serde_name(&variant_ident_str, variant_rename.as_deref(), naming.variants);
            let field_rename_all = parse_serde_rename_all(&variant.attrs).or(naming.fields);
            let variant_annotations = variant.variant_customizations(core);

            match variant.fields.style {
                darling::ast::Style::Struct => {
                    let object = crate::schematic::named_variant_object(core, None, variant.fields.fields, field_rename_all);
                    quote! {
                        {
                            let mut variant_object = #object;
                            variant_object.insert("title".to_string(), #core::serde_json::to_value(#variant_string).expect("cannot convert title to value"));
                            #( #variant_annotations )*
                            #core::serde_json::to_value(variant_object).expect("cannot convert variant to value")
                        }
                    }
                }
                darling::ast::Style::Unit => {
                    // A unit variant in an untagged enum serializes as null.
                    quote! {
                        {
                            let mut null_schema: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                            null_schema.insert("type".to_string(), #core::serde_json::to_value("null").expect("cannot convert type to value"));
                            #core::serde_json::to_value(null_schema).expect("cannot convert null schema to value")
                        }
                    }
                }
                darling::ast::Style::Tuple => {
                    if variant.fields.fields.len() == 1 {
                        // Single unnamed field (newtype): the inner type's schema directly, with
                        // the field's `#[schematic(..)]` annotations applied.
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
                        // Tuple variant `V(A, B)` (including `V()`) serializes as `[a, b]`.
                        crate::schematic::tuple_variant_schema(core, &variant.fields.fields)
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

                let branches: Vec<#core::serde_json::Value> = vec![
                    #(
                        #variants_codegen,
                    )*
                ];

                // untagged enum: oneOf without discriminator
                schema.schema.extras.insert("oneOf".to_string(), #core::serde_json::to_value(branches).unwrap());
                schema
            })
        }

        fn flatten_schema() -> Option<#core::serde_json::Value> {
            // Return the oneOf schema for flattening. A branch's `additionalProperties` would
            // also constrain the fields the parent declares next to the flattened enum, so it is
            // dropped from object branches.
            let mut branches: Vec<#core::serde_json::Value> = vec![
                #(
                    #variants_codegen,
                )*
            ];
            for branch in &mut branches {
                if let Some(object) = branch.as_object_mut() {
                    object.remove("additionalProperties");
                    object.remove("unevaluatedProperties");
                }
            }
            let mut obj: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
            obj.insert("oneOf".to_string(), #core::serde_json::to_value(branches).unwrap());
            Some(#core::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
