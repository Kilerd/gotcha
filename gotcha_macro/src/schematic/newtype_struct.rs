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
    let wire_fields = wire_fields_impl(core, &fields[0]);
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
    let name_impl = match name {
        Some(name) => quote! { #name },
        None => quote! { <#inner_ty as #core::Schematic>::name() },
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
    let doc = quote! { <#inner_ty as #core::Schematic>::doc() };
    delegating_impl(core, inner_ty, name_impl, doc, schema_impl, wire_fields, false)
}

/// Handle a container whose both directions convert through one proxy, which must implement
/// `Schematic`.
///
/// The container registers a component of its own, even if that is only a reference to the proxy.
/// So it keeps its name and documentation, and a proxy that contains the container (`Vec<Self>`)
/// ends in a reference rather than in endless recursion. The wire table stays the container's:
/// `Valid` reports errors under the fields of the converted value, not of the proxy.
pub(crate) fn proxy_handler(core: &TokenStream2, proxy: &syn::Type, name: &str, doc: TokenStream2, wire_fields: TokenStream2) -> TokenStream2 {
    let schema = quote! {
        #core::registry::schema_or_ref_for::<Self>(Self::schema_name(), module_path!(), Self::required(), || {
            let mut schema = <#proxy as #core::Schematic>::generate_schema();
            if let Some(doc) = Self::doc() {
                schema.schema.description = Some(doc);
            }
            schema
        })
    };
    delegating_impl(core, proxy, quote! { #name }, doc, schema, wire_fields, true)
}

/// The `wire_fields` of a type that is its only field on the wire.
pub(crate) fn wire_fields_impl(core: &TokenStream2, field: &ParameterStructFieldOpt) -> TokenStream2 {
    let inner_ty = field.schema_type();
    // A transparent named struct's field name is invisible on the wire (serde flattens the
    // inner value), but the remapper walks the Rust field name first: the entry tells it to
    // drop that segment and continue with the inner type's table.
    match field.ident.as_ref() {
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
    }
}

/// Every `Schematic` method of a type that is `inner_ty` on the wire, apart from its name, its
/// documentation, its schema and `wire_fields`.
fn delegating_impl(
    core: &TokenStream2, inner_ty: &syn::Type, name_impl: TokenStream2, doc_impl: TokenStream2, schema_impl: TokenStream2, wire_fields: TokenStream2,
    proxied: bool,
) -> TokenStream2 {
    // A proxy can contain the type, which a newtype's own field cannot do without `Box` on `main` too.
    let delegate = |method: &str, call: TokenStream2| {
        if proxied {
            quote! { #core::registry::unless_reentered::<Self, _>(#method, || #call) }
        } else {
            call
        }
    };
    let required = delegate("required", quote! { <#inner_ty as #core::Schematic>::required() });
    let nullable = delegate("nullable", quote! { <#inner_ty as #core::Schematic>::nullable() });
    let type_ = delegate("type_", quote! { <#inner_ty as #core::Schematic>::type_() });
    let format = delegate("format", quote! { <#inner_ty as #core::Schematic>::format() });
    let fields = delegate("fields", quote! { <#inner_ty as #core::Schematic>::fields() });
    let flatten_schema = delegate("flatten_schema", quote! { <#inner_ty as #core::Schematic>::flatten_schema() });
    let additional_properties = delegate("additional_properties", quote! { <#inner_ty as #core::Schematic>::additional_properties() });
    quote! {
        fn name() -> &'static str {
            #name_impl
        }
        fn required() -> bool {
            #required
        }
        fn nullable() -> Option<bool> {
            #nullable
        }
        fn type_() -> &'static str {
            #type_
        }
        fn doc() -> Option<String> {
            #doc_impl
        }
        fn format() -> Option<String> {
            #format
        }
        fn fields() -> Vec<(&'static str, #core::EnhancedSchema)> {
            #fields
        }
        fn generate_schema() -> #core::EnhancedSchema {
            #schema_impl
        }
        fn flatten_schema() -> Option<#core::serde_json::Value> {
            #flatten_schema
        }
        fn additional_properties() -> Option<#core::serde_json::Value> {
            #additional_properties
        }
        #wire_fields
    }
}
