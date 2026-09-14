use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::ParameterStructFieldOpt;

/// Handle a single-field tuple struct (a "newtype", e.g. `struct UserId(Uuid);`)
/// by delegating every `Schematic` method to the wrapped type. The newtype is
/// therefore transparent in the generated schema — `UserId` looks exactly like
/// `Uuid`, matching how serde serializes such wrappers.
pub(crate) fn handler(core: &TokenStream2, fields: Vec<ParameterStructFieldOpt>, name: Option<&str>) -> TokenStream2 {
    let inner_ty = &fields[0].ty;

    let name_impl = match name {
        Some(name) => quote! { #name },
        None => quote! { <#inner_ty as #core::Schematic>::name() },
    };
    let schema_impl = if name.is_some() {
        quote! {
            #core::registry::schema_or_ref_for::<Self>(Self::schema_name(), module_path!(), Self::required(), || {
                <#inner_ty as #core::Schematic>::generate_schema()
            })
        }
    } else {
        quote! { <#inner_ty as #core::Schematic>::generate_schema() }
    };
    quote! {
        fn name() -> &'static str {
            #name_impl
        }
        fn required() -> bool {
            <#inner_ty as #core::Schematic>::required()
        }
        fn nullable() -> Option<bool> {
            <#inner_ty as #core::Schematic>::nullable()
        }
        fn type_() -> &'static str {
            <#inner_ty as #core::Schematic>::type_()
        }
        fn doc() -> Option<String> {
            <#inner_ty as #core::Schematic>::doc()
        }
        fn format() -> Option<String> {
            <#inner_ty as #core::Schematic>::format()
        }
        fn fields() -> Vec<(&'static str, #core::EnhancedSchema)> {
            <#inner_ty as #core::Schematic>::fields()
        }
        fn generate_schema() -> #core::EnhancedSchema {
            #schema_impl
        }
        fn flatten_schema() -> Option<#core::serde_json::Value> {
            <#inner_ty as #core::Schematic>::flatten_schema()
        }
    }
}
