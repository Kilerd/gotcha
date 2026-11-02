use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;

/// How a branch documents the value a newtype variant wraps.
#[derive(Clone, Copy)]
pub(crate) enum WrappedValue {
    /// Through the value's own schema, usually a `$ref`.
    Referenced,
    /// A struct through its fields, as an internally tagged enum's other newtype variants do, so
    /// the struct does not become a component because of this enum.
    FieldsMerged,
}

/// Emit an expression of type `Value` for the branch of a variant written without a tag: a variant
/// of an untagged enum, or an `#[serde(untagged)]` variant of a tagged one. The surrounding code
/// has a `flattening` local saying whether the branch is composed into a parent's object.
pub(crate) fn variant_branch(core: &TokenStream2, variant: ParameterEnumVariantOpt, wrapped: WrappedValue) -> TokenStream2 {
    let variant_string = variant.serde_name();
    let variant_annotations = variant.variant_customizations(core);

    match variant.style() {
        darling::ast::Style::Struct => {
            let object = crate::schematic::named_variant_object(core, None, variant.fields.fields, true);
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
                let source = match wrapped {
                    WrappedValue::Referenced => quote! { #core::registry::wrapped_value_schema::<#inner_ty>(flattening) },
                    WrappedValue::FieldsMerged => quote! { #core::registry::wrapped_value_fields_schema::<#inner_ty>(flattening) },
                };
                quote! {
                    {
                        let mut field_schema = #source;
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
}

pub(crate) fn handler(core: &TokenStream2, ident_string: String, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>) -> syn::Result<TokenStream2> {
    let variants_codegen: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| variant_branch(core, variant, WrappedValue::Referenced))
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

                let flattening = false;
                let branches: Vec<#core::serde_json::Value> = vec![
                    #(
                        #variants_codegen,
                    )*
                ];

                // untagged enum: anyOf, because serde uses the first matching variant and several
                // branches can accept the same value
                schema.schema.extras.insert("anyOf".to_string(), #core::serde_json::to_value(branches).unwrap());
                schema
            })
        }

        fn flatten_schema() -> Option<#core::serde_json::Value> {
            // Return the anyOf schema for flattening. A branch's `additionalProperties` would
            // also constrain the fields the parent declares next to the flattened enum, so it is
            // dropped from object branches.
            let flattening = true;
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
            obj.insert("anyOf".to_string(), #core::serde_json::to_value(branches).unwrap());
            Some(#core::serde_json::to_value(obj).unwrap())
        }
    };

    Ok(ret)
}
