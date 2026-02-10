use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use crate::schematic::ParameterEnumVariantOpt;
use crate::utils::{get_serde_name, parse_serde_rename, RenameAll};

pub(crate) fn handler(
    ident: syn::Ident,
    doc: TokenStream2,
    variants: Vec<ParameterEnumVariantOpt>,
    rename_all: Option<RenameAll>,
) -> Result<TokenStream2, (Span, &'static str)> {
    let ident_string = ident.to_string();

    let variant_vec: Vec<TokenStream2> = variants
        .into_iter()
        .map(|variant| {
            let ident_str = variant.ident.to_string();
            let rename = parse_serde_rename(&variant.attrs);
            get_serde_name(&ident_str, rename.as_deref(), rename_all)
        })
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
