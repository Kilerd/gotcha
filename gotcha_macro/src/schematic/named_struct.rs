use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::schematic::ParameterStructFieldOpt;

pub(crate) fn handler(
    core: &TokenStream2, ident_string: String, doc: TokenStream2, fields: darling::ast::Fields<ParameterStructFieldOpt>, container_default: bool,
    tag: Option<(&str, &str)>, reads_another_type: bool,
) -> syn::Result<TokenStream2> {
    // `#[serde(tag = "..")]`: serde writes the tag next to the fields and ignores it when reading,
    // so this struct's own schema documents it without requiring it. It stays out of `fields()`, so
    // it is neither a query or path parameter nor part of a type that flattens this one. A field can
    // be written or read under the tag's name, which leaves the tag undocumented. So does a struct
    // with nothing in `fields()`, whose parameters come from these properties, and a flattened field
    // because that can read the name itself: the path for flattened fields never adds the tag.
    let own_tag = tag
        .filter(|(tag_name, _)| fields.fields.iter().any(|field| !field.serde.skipped()) && !fields.fields.iter().any(|field| field.serde.is_named(tag_name)))
        .map(|(tag_name, value)| {
            quote! { properties.insert(#tag_name.to_string(), #core::serde_json::json!({ "type": "string", "enum": [#value] })); }
        });

    let mut normal_fields_stream: Vec<TokenStream2> = Vec::new();
    let mut flatten_fields_stream: Vec<TokenStream2> = Vec::new();
    let mut flatten_schema_stream: Vec<TokenStream2> = Vec::new();
    let mut flatten_map_stream: Vec<TokenStream2> = Vec::new();
    let mut wire_fields_stream: Vec<TokenStream2> = Vec::new();

    for field in fields.fields.into_iter() {
        let ident_str = field.ident.as_ref().unwrap().to_string();

        // `skip` and `skip_deserializing` fields never arrive on the wire, so they get neither a
        // schema entry nor a wire-name entry.
        if field.serde.skip_deserializing {
            continue;
        }

        // A field skipped only for *serialization* is still deserialized, so its input name must
        // still be remapped. It stays out of the schema, and its type is not referenced at all,
        // so it does not newly need `Schematic` (the entry therefore carries no nested mapping).
        if field.serde.skip_serializing {
            let entry = if field.serde.flatten {
                quote! { #core::WireField::new(#ident_str, #ident_str).flatten() }
            } else {
                let wire_name = field.serde.wire_name();
                quote! { #core::WireField::new(#ident_str, #wire_name) }
            };
            wire_fields_stream.push(entry);
            continue;
        }

        let field_ty = field.schema_type().clone();

        if field.serde.flatten {
            // Flatten field: merge fields from the inner type at runtime. A flattened type that
            // is not required (an `Option<T>`, or a wrapper around one) may be absent, which
            // makes every field it contributes optional; otherwise their own requiredness holds.
            flatten_fields_stream.push(quote! {
                result.extend(<#field_ty as #core::Schematic>::fields().into_iter().map(|(name, mut schema)| {
                    if !<#field_ty as #core::Schematic>::required() {
                        schema.required = false;
                    }
                    (name, schema)
                }));
            });
            // Also collect flatten schemas for allOf generation
            flatten_schema_stream.push(quote! {
                if let Some(flatten_val) = <#field_ty as #core::Schematic>::flatten_schema() {
                    flatten_schemas.push(flatten_val);
                }
            });
            // A flattened map reports its value schema through `additional_properties()`;
            // non-map types return `None`, so nothing is generated or registered here.
            flatten_map_stream.push(quote! {
                if let Some(additional) = <#field_ty as #core::Schematic>::additional_properties() {
                    flatten_maps.push(additional);
                }
            });
            // The flattened type's fields appear at this level, so the entry carries the inner
            // mapping and `flatten: true` tells the remapper to drop this field's own segment.
            wire_fields_stream.push(quote! {
                #core::WireField::new(#ident_str, #ident_str)
                    .flatten()
                    .nested(<#field_ty as #core::Schematic>::wire_fields)
            });
        } else {
            // Normal field: static field entry
            let field_name = field.serde.schema_name();
            // Validation errors and parameters use the names serde accepts on input, which can
            // differ from the schema-side name. When serde reads another type this one's rename has
            // no effect on what it reads, so a parameter keeps the name in `fields()`.
            let wire_name = field.serde.wire_name();
            let schema_name = (!reads_another_type).then(|| quote! { .schema(#field_name) });
            wire_fields_stream.push(quote! {
                #core::WireField::new(#ident_str, #wire_name)
                    #schema_name
                    .nested(<#field_ty as #core::Schematic>::wire_fields)
            });
            // Presence and nullability come from the serde attributes already on the field, so
            // the schema matches what deserialization requires and serialization writes.
            let presence = field.serde.presence(&field_ty, container_default);
            let schema_ty = presence.schema_type;
            let required_override = match presence.required {
                Some(required) => quote! { field_schema.required = #required; },
                None => quote! {},
            };
            // Description (an explicit `#[schematic(description = ...)]` overrides the doc comment)
            // plus the `#[schematic(...)]` validation/metadata customizations.
            let (field_description, customizations) = field.schema_customizations(core);

            normal_fields_stream.push(quote! {
                (
                    #field_name,
                    {
                        let mut field_schema = <#schema_ty as #core::Schematic>::generate_schema();
                        field_schema.schema.description = #field_description;
                        #required_override
                        #( #customizations )*
                        field_schema
                    }
                )
            });
        }
    }

    let has_flatten = !flatten_schema_stream.is_empty();

    let generate_schema_impl = if has_flatten {
        // When there are flatten fields, we need to check at runtime if any
        // flatten field returns a non-None flatten_schema (i.e., is an enum)
        quote! {
            // Collect flatten schemas from enum types
            let mut flatten_schemas: Vec<#core::serde_json::Value> = vec![];
            #(
                #flatten_schema_stream
            )*
            // Flattened maps contribute `additionalProperties` (or `unevaluatedProperties` when
            // composing through allOf) instead of fields.
            let mut flatten_maps: Vec<#core::serde_json::Value> = vec![];
            #(
                #flatten_map_stream
            )*

            // Build the base object schema with own properties
            let mut required_fields = vec![];
            let mut properties = ::std::collections::HashMap::new();

            let fields = Self::fields();
            for (field_name, field_schema) in fields {
                properties.insert(field_name.to_string(), field_schema.schema.to_value());
                if field_schema.required {
                    required_fields.push(field_name.to_string());
                }
            }

            let combined_additional = match flatten_maps.len() {
                0 => None,
                1 => flatten_maps.pop(),
                _ => Some(#core::serde_json::json!({ "anyOf": flatten_maps })),
            };

            if flatten_schemas.is_empty() {
                // No enum flatten fields, just use simple object schema
                schema.schema._type = Some(Self::type_().into());
                schema.schema.extras.insert("properties".to_string(), #core::serde_json::to_value(properties).unwrap());
                schema.schema.extras.insert("required".to_string(), #core::serde_json::to_value(required_fields).unwrap());
                if let Some(additional) = combined_additional {
                    schema.schema.extras.insert("additionalProperties".to_string(), additional);
                }
            } else {
                // Has enum flatten fields, use allOf to combine
                let mut base_schema = ::std::collections::HashMap::new();
                base_schema.insert("type".to_string(), #core::serde_json::to_value("object").unwrap());
                base_schema.insert("properties".to_string(), #core::serde_json::to_value(properties).unwrap());
                base_schema.insert("required".to_string(), #core::serde_json::to_value(required_fields).unwrap());

                let mut all_of: Vec<#core::serde_json::Value> = vec![
                    #core::serde_json::to_value(base_schema).unwrap()
                ];
                all_of.extend(flatten_schemas);

                // With `allOf`, `additionalProperties` would also constrain the parent's own
                // declared fields; `unevaluatedProperties` applies only to entries no branch
                // evaluated (OpenAPI 3.2 supports it).
                if let Some(additional) = combined_additional {
                    schema.schema.extras.insert("unevaluatedProperties".to_string(), additional);
                }
                schema.schema._type = None;
                schema.schema.extras.insert("allOf".to_string(), #core::serde_json::to_value(all_of).unwrap());
            }
            schema
        }
    } else {
        // No flatten fields, use simple implementation
        quote! {
            let mut required_fields = vec![];
            let mut properties = ::std::collections::HashMap::new();

            let fields = Self::fields();
            for (field_name, field_schema) in fields {
                properties.insert(field_name.to_string(), field_schema.schema.to_value());
                if field_schema.required {
                    required_fields.push(field_name.to_string());
                }
            }
            #own_tag
            schema.schema.extras.insert("properties".to_string(), #core::serde_json::to_value(properties).unwrap());
            schema.schema.extras.insert("required".to_string(), #core::serde_json::to_value(required_fields).unwrap());
            schema
        }
    };

    // Generate flatten_schema implementation for structs
    // Flattened maps contribute `additionalProperties` at this level. A parent that flattens
    // this struct asks for the combined value through this method instead of generating a
    // schema for the struct (which would register a component during collection).
    let additional_properties_impl = if has_flatten {
        quote! {
            fn additional_properties() -> Option<#core::serde_json::Value> {
                let mut flatten_maps: Vec<#core::serde_json::Value> = vec![];
                #(
                    #flatten_map_stream
                )*
                match flatten_maps.len() {
                    0 => None,
                    1 => flatten_maps.pop(),
                    _ => Some(#core::serde_json::json!({ "anyOf": flatten_maps })),
                }
            }
        }
    } else {
        quote! {}
    };

    let flatten_schema_impl = if has_flatten {
        quote! {
            fn flatten_schema() -> Option<#core::serde_json::Value> {
                // Collect flatten schemas from inner types (for nested flatten)
                let mut flatten_schemas: Vec<#core::serde_json::Value> = vec![];
                #(
                    #flatten_schema_stream
                )*

                if flatten_schemas.is_empty() {
                    None
                } else if flatten_schemas.len() == 1 {
                    Some(flatten_schemas.remove(0))
                } else {
                    // Multiple enum flatten schemas - combine with allOf
                    let mut obj: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                    obj.insert("allOf".to_string(), #core::serde_json::to_value(flatten_schemas).unwrap());
                    Some(#core::serde_json::to_value(obj).unwrap())
                }
            }
        }
    } else {
        quote! {}
    };

    let ret = quote! {
        fn name() -> &'static str {
            #ident_string
        }

        fn required() -> bool {
            true
        }

        fn type_() -> &'static str {
            "object"
        }
        fn doc() -> Option<String> {
            #doc
        }
        fn fields() -> Vec<(&'static str, #core::EnhancedSchema)> {
            let mut result: Vec<(&'static str, #core::EnhancedSchema)> = vec![
                #(
                    #normal_fields_stream ,
                )*
            ];
            #(
                #flatten_fields_stream
            )*
            result
        }

        fn wire_fields() -> &'static [#core::WireField] {
            // The entries are built with `const fn` builders; the inline `const` block promotes
            // them into the `'static` table. This also works for generic structs, whose nested
            // entries depend on the type parameters.
            const {
                &[
                    #(
                        #wire_fields_stream ,
                    )*
                ]
            }
        }

        #flatten_schema_impl
        #additional_properties_impl

        fn generate_schema() -> #core::EnhancedSchema {
            // During spec assembly this registers the schema under `name()` and returns a `$ref`
            // to it (which is also what makes recursive types terminate); outside that scope it
            // just builds the schema inline.
            #core::registry::schema_or_ref_for::<Self>(Self::schema_name(), module_path!(), Self::required(), || {
                let mut schema = #core::EnhancedSchema {
                    schema: #core::oas::Schema {
                        _type: Some(Self::type_().into()),
                        format:None,
                        nullable:Self::nullable(),
                        description: Self::doc(),
                        extras: Default::default(),
                        ..#core::oas::Schema::default()
                    },
                    required: Self::required(),
                };

                #generate_schema_impl
            })
        }
    };

    Ok(ret)
}
