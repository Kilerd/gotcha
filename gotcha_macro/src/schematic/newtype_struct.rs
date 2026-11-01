use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::ParameterStructFieldOpt;

/// Handle a single-field tuple struct (a "newtype", e.g. `struct UserId(Uuid);`)
/// by delegating every `Schematic` method to the wrapped type. The newtype is
/// therefore transparent in the generated schema — `UserId` looks exactly like
/// `Uuid`, matching how serde serializes such wrappers.
pub(crate) fn handler(core: &TokenStream2, fields: Vec<ParameterStructFieldOpt>, name: Option<&str>) -> TokenStream2 {
    // `#[schematic(as = "Type")]` on the inner field replaces the type the newtype is transparent to.
    let inner_ty = fields[0].schema_type();

    let name_impl = match name {
        Some(name) => quote! { #name },
        None => quote! { <#inner_ty as #core::Schematic>::name() },
    };
    // A transparent named struct's field name is invisible on the wire (serde flattens the
    // inner value), but the remapper walks the Rust field name first: the entry tells it to
    // drop that segment and continue with the inner type's table.
    let wire_fields_impl = match fields[0].ident.as_ref() {
        Some(ident) => {
            let ident_string = ident.to_string();
            quote! {
                fn wire_fields() -> &'static [#core::WireField] {
                    const {
                        &[#core::WireField::new(#ident_string, #ident_string)
                            .flatten()
                            .nested(<#inner_ty as #core::Schematic>::wire_fields)]
                    }
                }
            }
        }
        None => quote! {
            fn wire_fields() -> &'static [#core::WireField] {
                <#inner_ty as #core::Schematic>::wire_fields()
            }
        },
    };
    // Apply the inner field's `#[schematic(..)]` documentation to the schema this newtype
    // delegates to. Only override the inner description when the field documents one; the inner
    // type's own documentation stays otherwise.
    // Only explicit attributes document this field; a doc comment here never did.
    let (field_description, customizations) = fields[0].value_schema_customizations(core);
    let annotated_schema = quote! {
        {
            let mut field_schema = <#inner_ty as #core::Schematic>::generate_schema();
            let description: Option<String> = #field_description;
            if let Some(description) = description {
                field_schema.schema.description = Some(description);
            }
            #( #customizations )*
            field_schema
        }
    };
    let schema_impl = if name.is_some() {
        quote! {
            #core::registry::schema_or_ref_for::<Self>(Self::schema_name(), module_path!(), Self::required(), || {
                #annotated_schema
            })
        }
    } else {
        annotated_schema
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
        fn additional_properties() -> Option<#core::serde_json::Value> {
            <#inner_ty as #core::Schematic>::additional_properties()
        }
        #wire_fields_impl
    }
}
