use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::{ParameterEnumVariantOpt, VariantNaming};

pub(crate) fn handler(
    core: &TokenStream2, ident_string: String, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>, naming: VariantNaming,
) -> syn::Result<TokenStream2> {
    let variant_vec: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| variant.serde_name(naming))
        .map(|variant_str| quote! { #variant_str })
        .collect();

    let ret = quote! {
        fn name() -> &'static str {
            #ident_string
        }

        fn required() -> bool {
            true
        }

        fn type_() -> &'static str {
            "string"
        }
        fn doc() -> Option<String> {
            #doc
        }
        fn generate_schema() -> #core::EnhancedSchema {
            #core::registry::schema_or_ref_for::<Self>(Self::schema_name(), module_path!(), Self::required(), || {
                let mut schema = #core::EnhancedSchema {
                    schema: #core::oas::Schema {
                        _type: Some(Self::type_().into()),
                        format:None,
                        nullable:None,
                        description: Self::doc(),
                        extras: Default::default(),
                        ..#core::oas::Schema::default()
                    },
                    required: Self::required(),
                };
                let enum_variants:Vec<&'static str> = vec![ #(#variant_vec ,)* ];
                schema.schema.extras.insert("enum".to_string(), #core::serde_json::to_value(enum_variants).unwrap());
                schema
            })
        }

        fn flatten_schema() -> Option<#core::serde_json::Value> {
            // Built inline rather than through `generate_schema`, which hands back a `$ref` during
            // spec assembly — a flattened enum has to merge its actual shape into the parent.
            let mut schema = #core::oas::Schema {
                _type: Some(Self::type_().into()),
                format:None,
                nullable:None,
                description: Self::doc(),
                extras: Default::default(),
                ..#core::oas::Schema::default()
            };
            let enum_variants:Vec<&'static str> = vec![ #(#variant_vec ,)* ];
            schema.extras.insert("enum".to_string(), #core::serde_json::to_value(enum_variants).unwrap());
            Some(schema.to_value())
        }
    };

    Ok(ret)
}
