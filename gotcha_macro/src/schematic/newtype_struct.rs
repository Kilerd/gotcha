use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::ParameterStructFieldOpt;

/// Handle a single-field tuple struct (a "newtype", e.g. `struct UserId(Uuid);`)
/// by delegating every `Schematic` method to the wrapped type. The newtype is
/// therefore transparent in the generated schema — `UserId` looks exactly like
/// `Uuid`, matching how serde serializes such wrappers.
pub(crate) fn handler(fields: Vec<ParameterStructFieldOpt>) -> TokenStream2 {
    let inner_ty = &fields[0].ty;

    quote! {
        fn name() -> &'static str {
            <#inner_ty as ::gotcha_core::Schematic>::name()
        }
        fn required() -> bool {
            <#inner_ty as ::gotcha_core::Schematic>::required()
        }
        fn nullable() -> Option<bool> {
            <#inner_ty as ::gotcha_core::Schematic>::nullable()
        }
        fn type_() -> &'static str {
            <#inner_ty as ::gotcha_core::Schematic>::type_()
        }
        fn doc() -> Option<String> {
            <#inner_ty as ::gotcha_core::Schematic>::doc()
        }
        fn format() -> Option<String> {
            <#inner_ty as ::gotcha_core::Schematic>::format()
        }
        fn fields() -> Vec<(&'static str, ::gotcha_core::EnhancedSchema)> {
            <#inner_ty as ::gotcha_core::Schematic>::fields()
        }
        fn generate_schema() -> ::gotcha_core::EnhancedSchema {
            <#inner_ty as ::gotcha_core::Schematic>::generate_schema()
        }
        fn flatten_schema() -> Option<::gotcha_core::serde_json::Value> {
            <#inner_ty as ::gotcha_core::Schematic>::flatten_schema()
        }
    }
}
