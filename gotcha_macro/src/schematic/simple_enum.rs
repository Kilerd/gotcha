use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;

pub(crate) fn handler(ident: syn::Ident, doc: TokenStream2, variants: Vec<ParameterEnumVariantOpt>) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();

    let variant_vec: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| variant.ident.to_string())
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
        fn generate_schema() -> ::gotcha::EnhancedSchema {
            let mut schema = ::gotcha::EnhancedSchema {
                schema: ::gotcha::oas::Schema {
                    _type: Some(Self::type_().to_string()),
                    format:None,
                    nullable:None,
                    description: Self::doc(),
                    extras:Default::default()
                },
                required: Self::required(),
            };
            let enum_variants:Vec<&'static str> = vec![ #(#variant_vec ,)* ];
            schema.schema.extras.insert("enum".to_string(), ::gotcha::serde_json::to_value(enum_variants).unwrap());
            schema
        }
    };

    Ok(ret)
}
