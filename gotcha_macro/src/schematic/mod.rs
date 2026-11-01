use darling::ast::Data;
use darling::{FromDeriveInput, FromField, FromVariant};
use proc_macro2::{Delimiter, Group, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::{parse2, DeriveInput};

pub mod adjacent_tagged_enum;
pub mod external_tagged_enum;
pub mod named_struct;
pub mod newtype_struct;
pub mod serde_model;
pub mod simple_enum;
pub mod tagged_enum;
pub mod untagged_enum;

use crate::utils::{expr_lit, AttributesExt};
use serde_model::{CaseRules, SerdeContainer, SerdeField, SerdeVariant, Tagging};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(schematic), forward_attrs(allow, doc, cfg))]
pub(crate) struct ParameterOpts {
    ident: syn::Ident,
    generics: syn::Generics,
    name: Option<String>,
    #[darling(rename = "crate")]
    crate_path: Option<syn::Path>,
    data: Data<ParameterEnumVariantOpt, ParameterStructFieldOpt>,
    attrs: Vec<syn::Attribute>,
}

impl ParameterOpts {
    /// Read the serde attributes of the variants and fields. Their names depend on the container's
    /// case conventions, so this runs on darling's finished parse.
    fn parse_serde(&mut self, container: &SerdeContainer) {
        match &mut self.data {
            Data::Enum(variants) => variants.iter_mut().for_each(|variant| variant.parse_serde(container)),
            Data::Struct(fields) => fields.fields.iter_mut().for_each(|field| field.parse_serde(container.rename_all)),
        }
    }
}

/// A literal JSON value (`= 42`, `= "x"`, `= true`) whose JSON type is preserved. Used for
/// `example` / `default` so a numeric example stays a number instead of becoming a string.
#[derive(Debug, Clone)]
struct SchemaValue(TokenStream2);

impl darling::FromMeta for SchemaValue {
    /// A negative number is a literal, as it was before syn 2: syn 2 parses `-1` as a literal
    /// only when it ends the attribute, and as a negation expression when an item follows.
    fn from_expr(expr: &syn::Expr) -> darling::Result<Self> {
        match expr {
            syn::Expr::Lit(lit) => Self::from_value(&lit.lit),
            syn::Expr::Group(group) => Self::from_expr(&group.expr),
            // `-$x` with a `macro_rules!` fragment negates an invisible group around the literal.
            syn::Expr::Unary(syn::ExprUnary {
                op: syn::UnOp::Neg(_),
                expr: inner,
                ..
            }) => match expr_lit(inner) {
                Some(lit) => Self::negative(lit),
                None => Err(darling::Error::unexpected_expr_type(expr)),
            },
            _ => Err(darling::Error::unexpected_expr_type(expr)),
        }
        .map_err(|error| error.with_span(expr))
    }

    fn from_value(value: &syn::Lit) -> darling::Result<Self> {
        let tokens = match value {
            syn::Lit::Str(v) => {
                let v = v.value();
                quote! { #v }
            }
            syn::Lit::Int(v) => {
                let v = v.base10_parse::<i64>().map_err(darling::Error::from)?;
                quote! { #v }
            }
            syn::Lit::Float(v) => {
                let v = v.base10_parse::<f64>().map_err(darling::Error::from)?;
                quote! { #v }
            }
            syn::Lit::Bool(v) => {
                let v = v.value;
                quote! { #v }
            }
            _ => return Err(darling::Error::unexpected_lit_type(value)),
        };
        Ok(SchemaValue(tokens))
    }
}

impl SchemaValue {
    /// `-<lit>` for a number literal, keeping the integer/float distinction of [`Self::from_value`].
    fn negative(lit: &syn::Lit) -> darling::Result<Self> {
        let tokens = match lit {
            syn::Lit::Int(v) => {
                let v = format!("-{}", v.base10_digits())
                    .parse::<i64>()
                    .map_err(|error| syn::Error::new(v.span(), error))?;
                quote! { #v }
            }
            syn::Lit::Float(v) => {
                let v = format!("-{}", v.base10_digits())
                    .parse::<f64>()
                    .map_err(|error| syn::Error::new(v.span(), error))?;
                quote! { #v }
            }
            _ => return Err(darling::Error::unexpected_lit_type(lit)),
        };
        Ok(SchemaValue(tokens))
    }
}

#[derive(Debug, FromField)]
#[darling(attributes(schematic), forward_attrs(allow, doc, cfg, serde, validate))]
pub(crate) struct ParameterStructFieldOpt {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    attrs: Vec<syn::Attribute>,
    /// The `#[serde(..)]` attributes among `attrs`, parsed once by [`ParameterOpts::parse_serde`].
    #[darling(skip)]
    serde: SerdeField,

    // `#[schematic(...)]` field customizations — pure schema *documentation* only. darling
    // treats `Option<_>` fields as optional automatically, so an absent attribute maps to `None`.
    //
    // Validation constraints (min/max, length, pattern, multiple_of, items, …) are deliberately
    // NOT here: those belong to `#[validation(...)]` (issue #9) as the single source of truth for
    // both runtime request validation and the schema, so a constraint is never written twice.
    title: Option<String>,
    description: Option<String>,
    example: Option<SchemaValue>,
    default: Option<SchemaValue>,
    format: Option<String>,
    /// `#[schematic(as = "Type")]`: describe the field as `Type` without changing the Rust
    /// field type. `Type` supplies the schema, requiredness/nullability, and wire-name mapping.
    #[darling(rename = "as")]
    as_type: Option<syn::Type>,
}

/// The subset of `#[validate(...)]` (validator crate) rules that map cleanly to JSON-Schema
/// keywords. Unmodelled validators (`regex`, `custom`, `must_match`, `contains`, …) are ignored
/// so they stay runtime-only without breaking the derive.
#[derive(Default)]
struct ValidateConstraints {
    minimum: Option<f64>,
    maximum: Option<f64>,
    /// Whether the corresponding bound came from `exclusive_min` / `exclusive_max`.
    exclusive_minimum: bool,
    exclusive_maximum: bool,
    /// `length(min/max/equal)` — recorded with the keywords of the schema's type at runtime.
    min_length: Option<u64>,
    max_length: Option<u64>,
    format: Option<&'static str>,
}

/// The items of a `#[validate(...)]` attribute, parsed leniently.
///
/// The validator crate accepts arguments that are not literals — `regex(path = *RE)`,
/// `length(max = MAX_LEN)` — whose values are unknown at expansion time. Rather than dropping the whole
/// attribute (and with it every constraint it declares), each list is rewritten to the literal
/// name-value items it contains, so the constraints that map to JSON Schema are still read.
fn validate_items(attr: &syn::Attribute) -> Vec<syn::Meta> {
    let syn::Meta::List(list) = &attr.meta else {
        return Vec::new();
    };
    if !matches!(list.delimiter, syn::MacroDelimiter::Paren(_)) {
        return Vec::new();
    }
    split_top_level_commas(list.tokens.clone()).into_iter().filter_map(lenient_meta).collect()
}

/// Split a token stream on its top-level commas. Commas inside a group are part of that group.
fn split_top_level_commas(tokens: TokenStream2) -> Vec<TokenStream2> {
    let mut items = Vec::new();
    let mut current = TokenStream2::new();
    for token in tokens {
        if matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ',') {
            if !current.is_empty() {
                items.push(std::mem::take(&mut current));
            }
        } else {
            current.extend([token]);
        }
    }
    if !current.is_empty() {
        items.push(current);
    }
    items
}

/// Parse one comma-separated item of `#[validate(...)]`, dropping the parts that are not
/// literal. A `name(args)` item with a non-literal argument keeps its literal arguments, so
/// `length(min = 1, max = MAX_LEN)` still yields `length(min = 1)`.
fn lenient_meta(tokens: TokenStream2) -> Option<syn::Meta> {
    if let Some(meta) = literal_meta(tokens.clone()) {
        return Some(meta);
    }
    let mut path_tokens = Vec::new();
    for part in tokens {
        let TokenTree::Group(group) = part else {
            path_tokens.push(part);
            continue;
        };
        if group.delimiter() != Delimiter::Parenthesis || path_tokens.is_empty() {
            return None;
        }
        let path: syn::Path = syn::parse2(path_tokens.iter().cloned().collect()).ok()?;
        let nested: Vec<syn::Meta> = split_top_level_commas(group.stream()).into_iter().filter_map(lenient_meta).collect();
        // No literal argument survived, so there is nothing to document.
        if nested.is_empty() {
            return None;
        }
        let nested_tokens = quote! { #(#nested),* };
        let mut rebuilt = quote! { #path };
        rebuilt.extend([TokenTree::Group(Group::new(Delimiter::Parenthesis, nested_tokens))]);
        return literal_meta(rebuilt);
    }
    None
}

/// `tokens` as a `Meta` whose arguments are all literals: a path, `name = <literal>`, or a
/// parenthesized list of such items and bare literals. Anything else is `None`.
///
/// syn 2 parses any expression after `=`, so the literal-only shape the schema mirrors is checked
/// here. List items are parsed one at a time, as syn 2 reads a negative number as a literal only
/// when nothing follows it.
fn literal_meta(tokens: TokenStream2) -> Option<syn::Meta> {
    let meta = syn::parse2::<syn::Meta>(tokens).ok()?;
    let literal = match &meta {
        syn::Meta::Path(_) => true,
        syn::Meta::NameValue(name_value) => expr_lit(&name_value.value).is_some(),
        syn::Meta::List(list) => {
            matches!(list.delimiter, syn::MacroDelimiter::Paren(_))
                && list_items(list.tokens.clone()).is_some_and(|items| {
                    items
                        .into_iter()
                        .all(|item| syn::parse2::<syn::Lit>(item.clone()).is_ok() || literal_meta(item).is_some())
                })
        }
    };
    literal.then_some(meta)
}

/// The comma-separated items of a list, or `None` when an item is empty. Only one trailing comma
/// is accepted, as in `Punctuated::parse_terminated`.
fn list_items(tokens: TokenStream2) -> Option<Vec<TokenStream2>> {
    let mut items = Vec::new();
    let mut current = TokenStream2::new();
    for token in tokens {
        if matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ',') {
            if current.is_empty() {
                return None;
            }
            items.push(std::mem::take(&mut current));
        } else {
            current.extend([token]);
        }
    }
    if !current.is_empty() {
        items.push(current);
    }
    Some(items)
}

impl ValidateConstraints {
    fn from_attrs(attrs: &[syn::Attribute]) -> Self {
        let mut c = ValidateConstraints::default();
        for attr in attrs {
            if !attr.path().is_ident("validate") {
                continue;
            }
            for item in validate_items(attr) {
                match item {
                    syn::Meta::List(list) if list.path.is_ident("range") => {
                        for (name, lit) in name_values(&list) {
                            match name.as_str() {
                                "min" => c.minimum = lit_to_f64(&lit),
                                "max" => c.maximum = lit_to_f64(&lit),
                                // JSON Schema stores exclusive bounds as numbers.
                                "exclusive_min" => {
                                    c.minimum = lit_to_f64(&lit);
                                    c.exclusive_minimum = true;
                                }
                                "exclusive_max" => {
                                    c.maximum = lit_to_f64(&lit);
                                    c.exclusive_maximum = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    syn::Meta::List(list) if list.path.is_ident("length") => {
                        for (name, lit) in name_values(&list) {
                            match name.as_str() {
                                "min" => c.min_length = lit_to_u64(&lit),
                                "max" => c.max_length = lit_to_u64(&lit),
                                // `equal` fixes both bounds.
                                "equal" => {
                                    c.min_length = lit_to_u64(&lit);
                                    c.max_length = lit_to_u64(&lit);
                                }
                                _ => {}
                            }
                        }
                    }
                    // `email` / `url` accept both the bare word and the `email(message = ...)` form.
                    syn::Meta::Path(p) if p.is_ident("email") => c.format = Some("email"),
                    syn::Meta::List(list) if list.path.is_ident("email") => c.format = Some("email"),
                    syn::Meta::Path(p) if p.is_ident("url") => c.format = Some("uri"),
                    syn::Meta::List(list) if list.path.is_ident("url") => c.format = Some("uri"),
                    _ => {}
                }
            }
        }
        c
    }
}

/// The `name = <lit>` pairs inside a `range(..)` / `length(..)` list.
fn name_values(list: &syn::MetaList) -> Vec<(String, syn::Lit)> {
    split_top_level_commas(list.tokens.clone())
        .into_iter()
        .filter_map(|item| match syn::parse2::<syn::Meta>(item).ok()? {
            syn::Meta::NameValue(name_value) => Some((name_value.path.get_ident()?.to_string(), expr_lit(&name_value.value)?.clone())),
            _ => None,
        })
        .collect()
}

fn lit_to_f64(lit: &syn::Lit) -> Option<f64> {
    match lit {
        syn::Lit::Int(i) => i.base10_parse().ok(),
        syn::Lit::Float(f) => f.base10_parse().ok(),
        _ => None,
    }
}

fn lit_to_u64(lit: &syn::Lit) -> Option<u64> {
    match lit {
        syn::Lit::Int(i) => i.base10_parse().ok(),
        _ => None,
    }
}

impl ParameterStructFieldOpt {
    fn parse_serde(&mut self, rules: CaseRules) {
        self.serde = SerdeField::parse(self.ident.as_ref(), &self.attrs, rules);
    }

    /// The type whose `Schematic` describes this field: the `#[schematic(as = "...")]` type when
    /// present, otherwise the declared field type. The original type is then never required to
    /// implement `Schematic`.
    pub(crate) fn schema_type(&self) -> &syn::Type {
        self.as_type.as_ref().unwrap_or(&self.ty)
    }

    /// Builds the `description` token and the list of `#[schematic(...)]` customization
    /// statements for this field. Each statement mutates a local `field_schema` binding
    /// (its `.schema.format` / `.schema.extras`). An explicit `#[schematic(description = "...")]`
    /// overrides the doc comment.
    pub(crate) fn schema_customizations(&self, core: &TokenStream2) -> (TokenStream2, Vec<TokenStream2>) {
        self.schema_customizations_with(core, true)
    }

    /// Customizations for a field used as a *value* (a tuple/newtype variant field or a
    /// newtype's inner field). Doc comments do not become descriptions here: they were never
    /// part of the document for these shapes, so turning them on would change correct documents.
    pub(crate) fn value_schema_customizations(&self, core: &TokenStream2) -> (TokenStream2, Vec<TokenStream2>) {
        self.schema_customizations_with(core, false)
    }

    fn schema_customizations_with(&self, core: &TokenStream2, include_doc: bool) -> (TokenStream2, Vec<TokenStream2>) {
        // Builds a statement that inserts a JSON-Schema keyword into `field_schema.schema.extras`.
        fn extra(core: &TokenStream2, key: &str, value: TokenStream2) -> TokenStream2 {
            quote! {
                field_schema.schema.extras.insert(#key.to_string(), #core::serde_json::to_value(#value).unwrap());
            }
        }

        let description = if let Some(desc) = &self.description {
            quote! { Some(#desc.to_string()) }
        } else if include_doc {
            if let Some(doc) = self.attrs.get_doc() {
                quote! { Some(#doc.to_string()) }
            } else {
                quote! { None }
            }
        } else {
            quote! { None }
        };

        let mut customizations: Vec<TokenStream2> = Vec::new();

        // Mirror `#[validate(...)]` constraints into the schema so a rule written once for runtime
        // validation also documents the field. Emitted first, so an explicit `#[schematic(...)]`
        // still wins on any overlap (e.g. `format`).
        let validated = ValidateConstraints::from_attrs(&self.attrs);
        if let Some(min) = validated.minimum {
            let key = if validated.exclusive_minimum { "exclusiveMinimum" } else { "minimum" };
            customizations.push(extra(core, key, quote! { #min }));
        }
        if let Some(max) = validated.maximum {
            let key = if validated.exclusive_maximum { "exclusiveMaximum" } else { "maximum" };
            customizations.push(extra(core, key, quote! { #max }));
        }
        // The keyword depends on the schema above, which is only known at runtime: a string
        // gets min/maxLength, an array min/maxItems and an object min/maxProperties.
        if validated.min_length.is_some() || validated.max_length.is_some() {
            let min = match validated.min_length {
                Some(min) => quote! { Some(#min) },
                None => quote! { None },
            };
            let max = match validated.max_length {
                Some(max) => quote! { Some(#max) },
                None => quote! { None },
            };
            customizations.push(quote! {
                #core::registry::apply_length_constraints(&mut field_schema.schema, #min, #max);
            });
        }
        if let Some(format) = validated.format {
            customizations.push(quote! { field_schema.schema.format = Some(#format.to_string()); });
        }

        if let Some(format) = &self.format {
            customizations.push(quote! { field_schema.schema.format = Some(#format.to_string()); });
        }
        if let Some(v) = &self.title {
            customizations.push(extra(core, "title", quote! { #v }));
        }
        // `example` / `default` keep their JSON type when converted from the parsed literal.
        if let Some(v) = &self.example {
            let value = &v.0;
            customizations.push(quote! { field_schema.schema.extras.insert("examples".to_string(), #core::serde_json::json!([#value])); });
        }
        if let Some(v) = &self.default {
            let value = &v.0;
            customizations.push(quote! { field_schema.schema.extras.insert("default".to_string(), #core::serde_json::Value::from(#value)); });
        }

        (description, customizations)
    }
}

/// Emit an expression for a tuple variant's array schema: one positional `prefixItems` entry
/// per field with the count pinned by `minItems`/`maxItems`.
///
/// External, adjacent and untagged tuple variants all serialize as an array of their fields;
/// one emitter keeps the three shapes documented identically. A `#[serde(skip)]` field is neither
/// written nor read, so it takes no position. A field skipped in one direction keeps its position:
/// serde writes and reads different arrays then, and the one schema cannot describe both.
pub(crate) fn tuple_variant_schema(core: &TokenStream2, fields: &[ParameterStructFieldOpt]) -> TokenStream2 {
    let field_schemas: Vec<TokenStream2> = fields
        .iter()
        .filter(|field| !field.serde.skipped_both_ways())
        .map(|field| {
            let field_ty = field.schema_type().clone();
            let (description, customizations) = field.value_schema_customizations(core);
            quote! {
                {
                    let mut field_schema = <#field_ty as #core::Schematic>::generate_schema();
                    let description: Option<String> = #description;
                    if let Some(description) = description {
                        field_schema.schema.description = Some(description);
                    }
                    #( #customizations )*
                    field_schema.schema.to_value()
                }
            }
        })
        .collect();
    quote! {
        {
            let prefix_items: Vec<#core::serde_json::Value> = vec![ #( #field_schemas ),* ];
            let item_count = prefix_items.len();
            let mut array_schema: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
            array_schema.insert("type".to_string(), #core::serde_json::to_value("array").expect("cannot convert type to value"));
            array_schema.insert("prefixItems".to_string(), #core::serde_json::to_value(prefix_items).expect("cannot convert prefixItems to value"));
            array_schema.insert("minItems".to_string(), #core::serde_json::to_value(item_count).expect("cannot convert minItems to value"));
            array_schema.insert("maxItems".to_string(), #core::serde_json::to_value(item_count).expect("cannot convert maxItems to value"));
            #core::serde_json::to_value(array_schema).expect("cannot convert array schema to value")
        }
    }
}

/// Emit an expression of type `HashMap<String, Value>` for one named-field variant object.
///
/// Every tag form builds the same object: the tag property (internally tagged only), the named
/// fields, and — when a field is `#[serde(flatten)]` — the flattened type's fields and composed
/// schema. One emitter keeps `skip`, `rename_all` / `rename_all_fields`, `flatten` and
/// `#[schematic(..)]` identical for externally, internally, adjacently and untagged enums
/// instead of drifting per copy.
///
/// `at_parent_level` says the object is the branch itself, not a body nested under a key. Such a
/// branch loses a flattened map's `additionalProperties` when its enum is flattened (where the
/// `flattening` local says so), so the value type must not be registered then.
///
/// `deny_unknown_fields` closes the object. A flattened field keeps it open: serde documents the
/// combination as unsupported, and a closed object would reject the flattened fields of an `allOf`
/// sibling. So does what leaves out a key serde writes or reads: a `skip_serializing` field, and a
/// field with a different name in each direction.
pub(crate) fn named_variant_object(
    core: &TokenStream2, tag: Option<(&str, &str)>, fields: Vec<ParameterStructFieldOpt>, at_parent_level: bool, deny_unknown_fields: bool,
) -> TokenStream2 {
    let closed = (deny_unknown_fields
        && !fields
            .iter()
            .any(|field| field.serde.flatten || (field.serde.skip_serializing && !field.serde.skip_deserializing) || field.serde.has_split_name()))
    .then(|| {
        quote! {
            base_object.insert("additionalProperties".to_string(), #core::serde_json::Value::Bool(false));
        }
    });
    let keeps_flattened_maps = if at_parent_level {
        quote! { !flattening }
    } else {
        quote! { true }
    };
    let tag_code = match tag {
        Some((tag_name, variant_name)) => quote! {
            {
                let mut tag_enum = ::std::collections::HashMap::new();
                tag_enum.insert("type".to_string(), #core::serde_json::to_value("string").expect("cannot convert type to value"));
                tag_enum.insert("enum".to_string(), #core::serde_json::to_value(vec![#variant_name.to_string()]).expect("cannot convert values to value"));
                properties.insert(#tag_name.to_string(), #core::serde_json::to_value(tag_enum).expect("cannot convert tag to value"));
                properties_required_fields.push(#tag_name.to_string());
            }
        },
        None => quote! {},
    };

    let field_statements: Vec<TokenStream2> = fields
        .into_iter()
        .filter_map(|field| {
            // `skip` / `skip_serializing` fields are not written; a field skipped only for
            // deserialization is still serialized and must stay in the schema.
            if field.serde.skip_serializing {
                return None;
            }
            let field_ty = field.schema_type().clone();
            if field.serde.flatten {
                // Keep the flattened type's own fields *and* its composed schema: the former for
                // structs, the latter for enums. A type that is not required (`Option<T>`) may be
                // absent, which makes the fields it contributes optional.
                return Some(quote! {
                    {
                        if let Some(flatten_val) = <#field_ty as #core::Schematic>::flatten_schema() {
                            flatten_schemas.push(flatten_val);
                        }
                        if #keeps_flattened_maps {
                            if let Some(additional) = <#field_ty as #core::Schematic>::additional_properties() {
                                flatten_maps.push(additional);
                            }
                        }
                        for (inner_name, mut inner_schema) in <#field_ty as #core::Schematic>::fields() {
                            if !<#field_ty as #core::Schematic>::required() {
                                inner_schema.required = false;
                            }
                            properties.insert(inner_name.to_string(), inner_schema.schema.to_value());
                            if inner_schema.required {
                                properties_required_fields.push(inner_name.to_string());
                            }
                        }
                    }
                });
            }
            let field_name = field.serde.schema_name();
            let presence = field.serde.presence(&field_ty, false);
            let schema_ty = presence.schema_type;
            let required_override = match presence.required {
                Some(required) => quote! { field_schema.required = #required; },
                None => quote! {},
            };
            let (field_description, customizations) = field.schema_customizations(core);
            Some(quote! {
                {
                    let mut field_schema = <#schema_ty as #core::Schematic>::generate_schema();
                    field_schema.schema.description = #field_description;
                    #required_override
                    #( #customizations )*
                    properties.insert(#field_name.to_string(), field_schema.schema.to_value());
                    if field_schema.required {
                        properties_required_fields.push(#field_name.to_string());
                    }
                }
            })
        })
        .collect();

    quote! {
        {
            let mut properties: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
            let mut properties_required_fields: Vec<String> = vec![];
            let mut flatten_schemas: Vec<#core::serde_json::Value> = vec![];
            let mut flatten_maps: Vec<#core::serde_json::Value> = vec![];
            #tag_code
            #( #field_statements )*
            let mut base_object: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
            base_object.insert("type".to_string(), #core::serde_json::to_value("object").expect("cannot convert type to value"));
            base_object.insert("properties".to_string(), #core::serde_json::to_value(properties).expect("cannot convert properties to value"));
            base_object.insert("required".to_string(), #core::serde_json::to_value(properties_required_fields).expect("cannot convert required fields to value"));
            #closed
            let combined_additional = match flatten_maps.len() {
                0 => None,
                1 => flatten_maps.pop(),
                _ => Some(#core::serde_json::json!({ "anyOf": flatten_maps })),
            };
            if flatten_schemas.is_empty() {
                if let Some(additional) = combined_additional {
                    base_object.insert("additionalProperties".to_string(), additional);
                }
                base_object
            } else {
                let mut all_of: Vec<#core::serde_json::Value> = vec![#core::serde_json::to_value(base_object).expect("cannot convert base object to value")];
                all_of.extend(flatten_schemas);
                let mut composed: ::std::collections::HashMap<String, #core::serde_json::Value> = ::std::collections::HashMap::new();
                if let Some(additional) = combined_additional {
                    composed.insert("unevaluatedProperties".to_string(), additional);
                }
                composed.insert("allOf".to_string(), #core::serde_json::to_value(all_of).expect("cannot convert allOf to value"));
                composed
            }
        }
    }
}

#[derive(Debug, FromVariant)]
#[darling(attributes(schematic), forward_attrs(allow, doc, cfg, serde))]
pub(crate) struct ParameterEnumVariantOpt {
    ident: syn::Ident,
    attrs: Vec<syn::Attribute>,
    /// The `#[serde(..)]` attributes among `attrs`, parsed once by [`ParameterOpts::parse_serde`].
    #[darling(skip)]
    serde: SerdeVariant,
    fields: darling::ast::Fields<ParameterStructFieldOpt>,
    // Variant-level schema documentation; a doc comment is the description fallback. Only
    // annotations that are valid on every branch shape (including unit/string branches) are
    // accepted: an `example`/`default` literal can never be valid for an object-shaped variant.
    title: Option<String>,
    description: Option<String>,
}

impl ParameterEnumVariantOpt {
    fn parse_serde(&mut self, container: &SerdeContainer) {
        let (serde, field_rules) = SerdeVariant::parse(&self.ident, &self.attrs, container);
        self.serde = serde;
        for field in &mut self.fields.fields {
            field.parse_serde(field_rules);
        }
    }

    /// Emit an expression for the branch of this `#[serde(untagged)]` variant of a tagged enum, as
    /// the `HashMap` the tagged handlers collect their branches in.
    pub(crate) fn untagged_branch_object(self, core: &TokenStream2, wrapped: untagged_enum::WrappedValue) -> TokenStream2 {
        let branch = untagged_enum::variant_branch(core, self, wrapped);
        quote! {
            #core::serde_json::from_value::<::std::collections::HashMap<String, #core::serde_json::Value>>(#branch).expect("cannot convert branch to object")
        }
    }

    /// The name the schema documents for this variant.
    pub(crate) fn serde_name(&self) -> String {
        self.serde.schema_name().to_owned()
    }

    /// The shape serde writes and reads this variant in: a newtype variant whose only field is
    /// `#[serde(skip)]` is a unit variant. Skipped in one direction, it is a unit variant in that
    /// direction only, so it keeps its documented field.
    pub(crate) fn style(&self) -> darling::ast::Style {
        match (self.fields.style, self.fields.fields.as_slice()) {
            (darling::ast::Style::Tuple, [field]) if field.serde.skipped_both_ways() => darling::ast::Style::Unit,
            (style, _) => style,
        }
    }

    /// Statements that add the variant's `#[schematic(..)]` documentation to `variant_object`.
    pub(crate) fn variant_customizations(&self, core: &TokenStream2) -> Vec<TokenStream2> {
        fn extra(core: &TokenStream2, key: &str, value: TokenStream2) -> TokenStream2 {
            quote! {
                variant_object.insert(#key.to_string(), #core::serde_json::to_value(#value).unwrap());
            }
        }

        let mut customizations = Vec::new();
        let description = self.description.clone().or_else(|| self.attrs.get_doc());
        if let Some(description) = description {
            customizations.push(extra(core, "description", quote! { #description }));
        }
        if let Some(title) = &self.title {
            customizations.push(extra(core, "title", quote! { #title }));
        }
        customizations
    }
}

// Resolve once per derive, then pass the same path through every schema generator.
// Libraries can stay on core alone; applications can use gotcha's re-export. Cargo
// dependency aliases are resolved by package name, including workspace dependencies.
fn core_crate_path() -> Option<TokenStream2> {
    if let Some(core) = crate::utils::crate_path("gotcha_core") {
        return Some(core);
    }
    let gotcha = crate::utils::crate_path("gotcha")?;
    Some(quote! { #gotcha::gotcha_core })
}

pub(crate) fn handler(input: TokenStream2) -> syn::Result<TokenStream2> {
    let x1 = parse2::<DeriveInput>(input)?;
    let mut param_opts = ParameterOpts::from_derive_input(&x1)?;
    if let Some(name) = &param_opts.name {
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')) {
            return Err(syn::Error::new(
                param_opts.ident.span(),
                "schema name must be nonempty and contain only ASCII letters, digits, '.', '-' or '_'",
            ));
        }
    }
    let core = match &param_opts.crate_path {
        Some(path) => quote! { #path },
        None => core_crate_path().ok_or_else(|| {
            syn::Error::new(
                param_opts.ident.span(),
                "Schematic requires a gotcha_core or gotcha dependency; use #[schematic(crate = \"path::to::gotcha_core\")] for a custom re-export",
            )
        })?,
    };
    let schema_name = param_opts.name.clone().unwrap_or_else(|| param_opts.ident.to_string());
    let name_override = match &param_opts.name {
        Some(name) => quote! { Some(#name) },
        None => quote! { None },
    };
    let serde = SerdeContainer::from_input(&x1);
    param_opts.parse_serde(&serde);
    let ident = param_opts.ident.clone();
    let doc = match param_opts.attrs.get_doc() {
        None => {
            quote! { None }
        }
        Some(t) => {
            quote! {Some( #t.to_owned()) }
        }
    };

    let (generics, generics_single, where_clause) = param_opts.generics.split_for_impl();

    let impl_stream = match (param_opts.data, &serde.proxy) {
        // Both directions convert through one proxy, so it is what is on the wire and its `Schematic`
        // documents the type. The wire table stays the type's own.
        (data, Some(proxy)) => {
            let wire_fields = match data {
                Data::Enum(_) => quote! {},
                Data::Struct(fields) => match fields.style {
                    darling::ast::Style::Struct => named_struct::wire_fields_impl(&core, &fields.fields, true),
                    _ if fields.fields.len() == 1 => newtype_struct::wire_fields_impl(&core, &fields.fields[0]),
                    _ => quote! {},
                },
            };
            newtype_struct::proxy_handler(&core, proxy, &schema_name, doc, wire_fields)
        }
        (Data::Enum(mut enum_variants), None) => {
            // A variant serde neither writes nor reads is not on the wire. One skipped in a
            // single direction stays: a branch the other direction never uses invalidates
            // nothing, while dropping it would hide a shape that direction does use.
            let declared_variants = enum_variants.len();
            let all_fieldless = enum_variants.iter().all(|variant| variant.fields.is_empty());
            enum_variants.retain(|variant| !variant.serde.skipped);
            let all_skipped = declared_variants > 0 && enum_variants.is_empty();
            // An all-unit enum is a plain string enum only in the default, externally tagged
            // representation. A tag turns it into an object and `untagged` writes null, so the
            // shape must come from the tag handler, not from the variant count. Removing skipped
            // variants can leave only unit variants; a zero-field `A {}` or `A()` is not one.
            let only_units = enum_variants.iter().all(|variant| matches!(variant.fields.style, darling::ast::Style::Unit));
            let is_simple_enum = (all_fieldless || only_units) && !enum_variants.iter().any(|variant| variant.serde.untagged);
            match serde.tagging {
                // No value is on the wire. The empty string enum says so; an empty `oneOf` or
                // `anyOf` is not a valid schema.
                _ if all_skipped => simple_enum::handler(&core, schema_name.clone(), doc, enum_variants)?,
                Tagging::External if is_simple_enum => simple_enum::handler(&core, schema_name.clone(), doc, enum_variants)?,
                Tagging::External => external_tagged_enum::handler(&core, schema_name.clone(), doc, enum_variants)?,
                Tagging::Internal(tag_name) => tagged_enum::handler(&core, schema_name.clone(), doc, enum_variants, tag_name)?,
                Tagging::Adjacent { tag, content } => adjacent_tagged_enum::handler(&core, schema_name.clone(), doc, enum_variants, tag, content)?,
                Tagging::Untagged => untagged_enum::handler(&core, schema_name.clone(), doc, enum_variants)?,
            }
        }
        (Data::Struct(fields), None) => {
            let is_tuple = matches!(fields.style, darling::ast::Style::Tuple);
            let field_count = fields.fields.len();
            // `#[serde(transparent)]` serializes its single non-skipped field; extra
            // `#[serde(skip)]` fields (the typed-ID `PhantomData` pattern) are allowed.
            let transparent_index = if serde.transparent && !is_tuple {
                let mut kept = fields.fields.iter().enumerate().filter(|(_, field)| !field.serde.skipped());
                match (kept.next(), kept.next()) {
                    (Some((index, _)), None) => Some(index),
                    _ => None,
                }
            } else {
                None
            };
            if is_tuple && field_count == 1 {
                // Newtype struct (e.g. `struct UserId(Uuid);`) — transparent to the inner type.
                newtype_struct::handler(&core, fields.fields, param_opts.name.as_deref())
            } else if let Some(index) = transparent_index {
                let mut fields = fields.fields;
                let field = fields.swap_remove(index);
                newtype_struct::handler(&core, vec![field], param_opts.name.as_deref())
            } else if is_tuple {
                return Err(syn::Error::new(
                    ident.span(),
                    "#[derive(Schematic)] does not support multi-field tuple structs; use a named struct",
                ));
            } else {
                // With `deny_unknown_fields` serde rejects a request carrying the tag it writes, and
                // a conversion replaces what serde writes or reads, so neither documents the tag.
                let tag = match &serde.tagging {
                    Tagging::Internal(tag) if !serde.deny_unknown_fields && !serde.converts => Some((tag.as_str(), serde.serialize_name.as_str())),
                    _ => None,
                };
                named_struct::handler(&core, schema_name.clone(), doc, fields, &serde, tag)?
            }
        }
    };

    let ret = quote! {
        impl #generics #core::Schematic for #ident #generics_single #where_clause {
            fn schema_name() -> Option<&'static str> { #name_override }
            #impl_stream
        }
    };

    Ok(ret)
}

#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn component_names_reject_invalid_openapi_keys() {
        for name in ["", "Bad/Name", "Bad~Name", "Bad Name", "Bad<Name>"] {
            let error = handler(quote! { #[schematic(name = #name)] struct Example { value: String } }).unwrap_err();
            assert!(error.to_string().contains("schema name must be nonempty"));
        }
        assert!(handler(quote! { #[schematic(crate = "::gotcha_core", name = "public.Result-v1")] struct Example { value: String } }).is_ok());
    }

    #[test]
    fn misspelled_container_options_report_compile_errors() {
        let error = handler(quote! { #[schematic(nmae = "Example")] struct Example { value: String } }).unwrap_err();
        assert!(error.to_string().contains("Unknown field: `nmae`"));
    }
}

#[cfg(test)]
mod validate_tests {
    use super::*;

    #[test]
    fn literal_constraints_survive_non_literal_arguments() {
        let attrs: Vec<syn::Attribute> = vec![syn::parse_quote!(#[validate(length(min = 1, max = MAX_LEN))])];
        let constraints = ValidateConstraints::from_attrs(&attrs);
        assert_eq!(constraints.min_length, Some(1));
        assert_eq!(constraints.max_length, None);
    }

    #[test]
    fn unmodelled_non_literal_validators_are_skipped() {
        let attrs: Vec<syn::Attribute> = vec![syn::parse_quote!(#[validate(regex(path = *NAME_RE), range(min = 1, max = 5))])];
        let constraints = ValidateConstraints::from_attrs(&attrs);
        assert_eq!(constraints.minimum, Some(1.0));
        assert_eq!(constraints.maximum, Some(5.0));
    }

    #[test]
    fn attributes_that_syn_cannot_parse_do_not_abort_the_derive() {
        let output = handler(quote! {
            #[schematic(crate = "::gotcha_core")]
            #[doc = include_str!("README.md")]
            struct Example {
                #[validate(regex(path = *NAME_RE))]
                #[validate(length(min = 1, max = MAX_LEN))]
                #[doc = include_str!("README.md")]
                value: String,
            }
        })
        .unwrap();
        assert!(output.to_string().contains("Schematic"));
    }
}

#[cfg(test)]
mod schema_value_tests {
    use super::*;

    fn derive_with_example(value: TokenStream2) -> syn::Result<TokenStream2> {
        handler(quote! {
            #[schematic(crate = "::gotcha_core")]
            struct Example {
                #[schematic(example = #value, title = "t")]
                value: i64,
            }
        })
    }

    #[test]
    fn only_number_literals_are_negated() {
        for value in [quote!(--1), quote!(-"x"), quote!(-true), quote!(-LIMIT)] {
            assert!(derive_with_example(value.clone()).is_err(), "{value}");
        }
        // A `macro_rules!` fragment arrives in an invisible group, which is looked through.
        let fragment = Group::new(Delimiter::None, quote!(7));
        let output = derive_with_example(quote!(-#fragment)).unwrap().to_string().replace(' ', "");
        assert!(output.contains("[-7i64]"), "{output}");
    }
}
