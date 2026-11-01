use std::str::FromStr;

use proc_macro::TokenStream;
use proc_macro2::{Group, Span, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::spanned::Spanned;
use syn::{FnArg, ItemFn, ReturnType};

use darling::ast::NestedMeta;
use darling::util::Flag;

use crate::utils::{gotcha_crate_path, AttributesExt};
use crate::FromMeta;

/// Parse a Rust type from a string literal.
///
/// Every token is re-spanned to `lit.span().resolved_at(Span::call_site())`: diagnostics still
/// point at the string, while the generated tokens resolve at the macro call site, so user-crate
/// lints (for example `unused_qualifications`) do not fire on macro output.
fn parse_type(lit: &syn::LitStr) -> darling::Result<syn::Type> {
    let span = lit.span().resolved_at(Span::call_site());
    let tokens = TokenStream2::from_str(&lit.value()).map_err(|error| darling::Error::from(syn::Error::new(span, error)))?;
    syn::parse2::<syn::Type>(respan(tokens, span)).map_err(|error| darling::Error::from(syn::Error::new(span, error)))
}

/// Replace the span of every token in `stream`, including nested groups, with `span`.
fn respan(stream: TokenStream2, span: Span) -> TokenStream2 {
    stream
        .into_iter()
        .map(|mut token| {
            if let TokenTree::Group(group) = &mut token {
                let inner = respan(group.stream(), span);
                *group = Group::new(group.delimiter(), inner);
            }
            token.set_span(span);
            token
        })
        .collect()
}

#[derive(Debug, FromMeta)]
pub struct RouteMeta {
    group: Option<String>,
    id: Option<String>,
    /// Explicit operation summary; when absent it is derived from the id in Title Case.
    summary: Option<String>,
    /// Marks the operation deprecated (`#[api(deprecated)]`).
    deprecated: Flag,
    /// Name of a security scheme required for this operation (empty scopes).
    security: Option<String>,
    responses: Option<ResponseDeclarations>,
    errors: Option<ErrorDeclarations>,
    drop_default: Flag,
    /// Documents only the explicit `responses(...)` contract; the return type is never referenced.
    skip_response: Flag,
}

impl RouteMeta {
    /// Reject option combinations whose meaning would be ambiguous or empty.
    fn validate(&self) -> darling::Result<()> {
        if !self.skip_response.is_present() {
            return Ok(());
        }
        let span = self.skip_response.span();
        if self.errors.is_some() {
            return Err(darling::Error::custom("skip_response cannot be combined with errors(...); declare every status in responses(...)").with_span(&span));
        }
        if self.drop_default.is_present() {
            return Err(darling::Error::custom("drop_default has no effect with skip_response; remove it").with_span(&span));
        }
        let has_responses = self.responses.as_ref().is_some_and(|responses| !responses.0.is_empty());
        if !has_responses {
            return Err(darling::Error::custom("skip_response requires at least one responses(response(...)) declaration").with_span(&span));
        }
        Ok(())
    }
}

/// `request_body(...)` may appear more than once, which `RouteMeta`'s single-value fields cannot
/// express, so the entries are split off before the rest is parsed.
fn split_request_bodies(args: Vec<NestedMeta>) -> darling::Result<(Vec<RequestBodyMeta>, Vec<NestedMeta>)> {
    let mut declarations = Vec::new();
    let mut remaining = Vec::new();
    for arg in args {
        match arg {
            // Match every form so a malformed `request_body` is reported as such rather than as
            // an unknown field of the route attribute.
            NestedMeta::Meta(meta) if meta.path().is_ident("request_body") => {
                declarations.push(RequestBodyMeta::from_meta(&meta)?);
            }
            other => remaining.push(other),
        }
    }
    Ok((declarations, remaining))
}

/// Entries describe one logical body, so they may only differ in media type and body type.
fn validate_request_bodies(declarations: &[RequestBodyMeta]) -> darling::Result<()> {
    let Some(first) = declarations.first() else {
        return Ok(());
    };
    let mut content_types = std::collections::BTreeSet::new();
    for declaration in declarations {
        if !content_types.insert(declaration.content_type.as_str()) {
            return Err(darling::Error::custom(format!(
                "duplicate request_body content_type {:?}; every entry must declare a distinct media type",
                declaration.content_type
            ))
            .with_span(&declaration.span));
        }
        if declaration.required != first.required {
            return Err(darling::Error::custom("all request_body(...) declarations must agree on required").with_span(&declaration.span));
        }
        if declaration.description != first.description {
            return Err(darling::Error::custom("all request_body(...) declarations must agree on description").with_span(&declaration.span));
        }
    }
    Ok(())
}

/// One media type of an explicitly declared request body.
#[derive(Debug)]
struct RequestBodyMeta {
    /// Span of the whole entry, used for errors that depend on the other entries.
    span: proc_macro2::Span,
    body: syn::Type,
    /// Defaults to `application/json`, matching a body declaration in `response(...)`.
    content_type: String,
    description: Option<String>,
    /// Defaults to `true`: a declared body is required unless stated otherwise.
    required: bool,
}

#[derive(Debug, FromMeta)]
struct RequestBodyFields {
    /// Kept as the literal so a malformed type is reported at the string, not the attribute.
    body: syn::LitStr,
    content_type: Option<String>,
    description: Option<String>,
    required: Option<bool>,
}

impl FromMeta for RequestBodyMeta {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        let syn::Meta::List(meta) = item else {
            return Err(darling::Error::custom("expected request_body(body = \"Type\")").with_span(item));
        };
        let fields = NestedMeta::parse_meta_list(meta.tokens.clone())
            .map_err(darling::Error::from)
            .and_then(|items| RequestBodyFields::from_list(&items))
            .map_err(|error| error.with_span(meta))?;
        Ok(Self {
            span: meta.span(),
            body: parse_type(&fields.body)?,
            content_type: fields.content_type.unwrap_or_else(|| "application/json".to_owned()),
            description: fields.description,
            required: fields.required.unwrap_or(true),
        })
    }
}

impl RequestBodyMeta {
    fn tokens(&self, gotcha: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let body = &self.body;
        let content_type = &self.content_type;
        let required = self.required;
        let description = match &self.description {
            Some(description) => quote!(Some(#description)),
            None => quote!(None),
        };
        quote!(#gotcha::request::request_body::<#body>(#content_type, #description, #required))
    }

    /// A constructor that builds the whole declared body, or `None` when nothing is declared.
    ///
    /// It is appended after the extractor constructors, so it replaces any body they infer.
    fn constructor(declarations: &[RequestBodyMeta], gotcha: &proc_macro2::TokenStream) -> Option<proc_macro2::TokenStream> {
        let mut entries = declarations.iter().map(|declaration| declaration.tokens(gotcha));
        let first = entries.next()?;
        let rest = entries;
        Some(quote! {
            |_url: ::std::string::String| {
                let mut body = #first;
                #(#gotcha::request::merge_request_bodies(&mut body, #rest);)*
                #gotcha::Either::Right(body)
            }
        })
    }
}

#[derive(Debug, Default)]
struct ResponseDeclarations(Vec<ResponseMeta>);

#[derive(Debug)]
struct ErrorDeclarations(Vec<ResponseMeta>);

impl FromMeta for ErrorDeclarations {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        let ResponseDeclarations(responses) = ResponseDeclarations::from_list(items)?;
        if responses.is_empty() {
            return Err(darling::Error::custom("errors(...) must declare at least one response"));
        }
        Ok(Self(responses))
    }
}

/// The arguments of one `response(...)` entry as written.
#[derive(Debug, FromMeta)]
struct ResponseFields {
    status: u16,
    /// Kept as the literal so a malformed type is reported at the string, not the attribute.
    body: Option<syn::LitStr>,
    /// OpenAPI 3.2 `itemSchema` for sequential media types such as `text/event-stream`.
    item: Option<syn::LitStr>,
    content_type: Option<String>,
    description: Option<String>,
}

/// One validated `response(...)` declaration, with its types parsed.
#[derive(Debug)]
struct ResponseMeta {
    status: u16,
    body: Option<syn::Type>,
    item: Option<syn::Type>,
    content_type: Option<String>,
    description: Option<String>,
}

impl FromMeta for ResponseDeclarations {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        let mut responses = Vec::new();
        let mut statuses = std::collections::BTreeSet::new();
        for item in items {
            let NestedMeta::Meta(syn::Meta::List(meta)) = item else {
                return Err(darling::Error::custom("expected response(status = ..., body = \"Type\")").with_span(item));
            };
            if !meta.path.is_ident("response") {
                return Err(darling::Error::custom("expected response(...)").with_span(meta));
            }
            let response = ResponseFields::from_list(&NestedMeta::parse_meta_list(meta.tokens.clone())?)?;
            if !(100..=599).contains(&response.status) {
                return Err(darling::Error::custom("response status must be in 100..=599").with_span(meta));
            }
            if !statuses.insert(response.status) {
                return Err(darling::Error::custom("duplicate response status").with_span(meta));
            }
            let body = response.body.as_ref().map(parse_type).transpose()?;
            let item = response.item.as_ref().map(parse_type).transpose()?;
            if item.is_some() && response.content_type.is_none() {
                return Err(darling::Error::custom("item requires an explicit content_type").with_span(meta));
            }
            let has_content = body.is_some() || item.is_some();
            if has_content && (response.status < 200 || matches!(response.status, 204 | 205 | 304)) {
                return Err(darling::Error::custom("this status cannot declare response content").with_span(meta));
            }
            if !has_content && response.content_type.is_some() {
                return Err(darling::Error::custom("content_type requires a body or item type").with_span(meta));
            }
            responses.push(ResponseMeta {
                status: response.status,
                body,
                item,
                content_type: response.content_type,
                description: response.description,
            });
        }
        Ok(Self(responses))
    }
}

impl ResponseMeta {
    fn tokens(&self, gotcha: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let status = self.status;
        let description = self.description.clone().unwrap_or_else(|| format!("HTTP {status}"));
        // Validation requires an explicit `content_type` for an item, so the default only
        // applies to a body.
        let media_type = self.content_type.as_deref().unwrap_or("application/json");
        match (&self.body, &self.item) {
            (Some(body), Some(item)) => quote!(#gotcha::response::response_with_item::<#body, #item>(#status, #media_type, #description)),
            (Some(body), None) => quote!(#gotcha::response::response::<#body>(#status, #media_type, #description)),
            (None, Some(item)) => quote!(#gotcha::response::item_response::<#item>(#status, #media_type, #description)),
            (None, None) => quote!(#gotcha::response::empty_response(#status, #description)),
        }
    }
}

/// Validate a parameter-level `#[api(...)]` attribute. `#[api(skip)]` is the only supported
/// form, and it is parsed rather than matched by substring.
fn validate_parameter_skip(attr: &syn::Attribute) -> syn::Result<()> {
    match &attr.meta {
        syn::Meta::List(meta) if meta.path.is_ident("api") && matches!(meta.delimiter, syn::MacroDelimiter::Paren(_)) => {
            let nested = meta
                .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
                .map_err(|_| unknown_parameter_attribute(meta))?;
            let mut nested = nested.iter();
            match (nested.next(), nested.next()) {
                (Some(syn::Meta::Path(path)), None) if path.is_ident("skip") => Ok(()),
                _ => Err(unknown_parameter_attribute(meta)),
            }
        }
        meta => Err(unknown_parameter_attribute(meta)),
    }
}

fn unknown_parameter_attribute(tokens: &impl quote::ToTokens) -> syn::Error {
    syn::Error::new_spanned(tokens, "unknown parameter-level #[api(...)]; only #[api(skip)] is supported")
}

/// Parse and validate the arguments of `#[api(...)]`.
fn parse_route_args(args: Vec<NestedMeta>) -> darling::Result<(RouteMeta, Vec<RequestBodyMeta>)> {
    let (request_bodies, args) = split_request_bodies(args)?;
    let meta = RouteMeta::from_list(&args)?;
    meta.validate()?;
    validate_request_bodies(&request_bodies)?;
    Ok((meta, request_bodies))
}

/// Add `error` to the errors one expansion reports.
fn push_error(errors: &mut Option<syn::Error>, error: syn::Error) {
    match errors {
        Some(errors) => errors.combine(error),
        None => *errors = Some(error),
    }
}

pub(crate) fn request_handler(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    expand_request_handler(args.into(), input_stream.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_request_handler(args: TokenStream2, input_stream: TokenStream2) -> syn::Result<TokenStream2> {
    let gotcha = gotcha_crate_path();
    let (meta, request_bodies) = parse_route_args(NestedMeta::parse_meta_list(args)?)?;
    let skip_response = meta.skip_response.is_present();
    let declared_request_body = RequestBodyMeta::constructor(&request_bodies, &gotcha);
    let extra_responses: Vec<_> = meta.responses.unwrap_or_default().0.iter().map(|response| response.tokens(&gotcha)).collect();
    let explicit_errors = meta.errors.is_some();
    let error_responses: Vec<_> = meta
        .errors
        .into_iter()
        .flat_map(|errors| errors.0)
        .map(|response| response.tokens(&gotcha))
        .collect();
    let drop_default = meta.drop_default.is_present();
    let group = if let Some(group_name) = meta.group {
        quote! { Some(#group_name) }
    } else {
        quote! { None }
    };
    let summary = if let Some(summary) = meta.summary {
        quote! { Some(#summary) }
    } else {
        quote! { None }
    };
    let deprecated = meta.deprecated.is_present();
    let security = if let Some(security) = meta.security {
        quote! { Some(#security) }
    } else {
        quote! { None }
    };
    let mut input = syn::parse2::<ItemFn>(input_stream)?;

    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();

    let operation_id = meta.id.unwrap_or(fn_ident_string.clone());

    let docs = match input.attrs.get_doc() {
        None => {
            quote!(None)
        }
        Some(t) => {
            quote! { Some(#t) }
        }
    };

    let mut params_token: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut extractor_tokens: Vec<proc_macro2::TokenStream> = Vec::new();
    // Every parameter is checked before reporting, so all misspelled attributes show at once.
    let mut parameter_errors = None;
    for param in &input.sig.inputs {
        let FnArg::Typed(typed) = param else {
            continue;
        };
        let mut skip = false;
        for attr in typed.attrs.iter().filter(|attr| attr.path().is_ident("api")) {
            if let Err(error) = validate_parameter_skip(attr) {
                push_error(&mut parameter_errors, error);
            }
            skip = true;
        }
        if skip {
            continue;
        }
        let ty = &typed.ty;
        if declared_request_body.is_some() {
            // The declared body replaces whatever a body extractor would infer, without
            // registering the replaced type's schema. The extractor still runs, so its rejection
            // responses are still documented.
            params_token.push(quote! { #gotcha::openapi::parameters_only::<#ty> });
        } else {
            params_token.push(quote! { <#ty as #gotcha::ParameterProvider>::generate });
        }
        extractor_tokens.push(quote! { <#ty as #gotcha::ParameterProvider>::extractor_kinds });
    }
    if let Some(error) = parameter_errors {
        return Err(error);
    }
    let ret_type = match &input.sig.output {
        // A handler without an explicit return type returns `()`.
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let ret_responses = if skip_response {
        // The return type is deliberately never mentioned: opaque types such as
        // `Sse<impl Stream<...>>` cannot be used in a trait path.
        quote!(#gotcha::oas::Responses {
            default: None,
            data: ::std::collections::BTreeMap::new()
        })
    } else if explicit_errors {
        quote!(<#ret_type as #gotcha::response::ResultResponse>::success_responses())
    } else {
        quote!(<#ret_type as #gotcha::Responsible>::response())
    };

    input.sig.inputs.iter_mut().for_each(|param| {
        if let FnArg::Typed(typed) = param {
            // Remove only the #[api(...)] attributes, keep others
            typed.attrs.retain(|attr| !attr.path().is_ident("api"));
        }
    });

    let ret = quote! {

        #input

        #gotcha::inventory::submit! {
            #gotcha::Operable {
                type_name: concat!(module_path!(), "::", #fn_ident_string),
                id: #operation_id,
                group: #group,
                summary: #summary,
                description: #docs,
                deprecated: #deprecated,
                security: #security,
                parameters: &[#(#params_token,)* #declared_request_body],
                extractors: &[#(#extractor_tokens,)*],
                responses: || {
                    let mut responses = #ret_responses;
                    // Declared Err alternatives join the inferred Ok contract, including shared statuses.
                    #(#gotcha::response::merge_responses(&mut responses, #error_responses);)*
                    // Explicit declarations replace inference for their status only.
                    #(responses.data.extend((#extra_responses).data);)*
                    if #drop_default { responses.default = None; }
                    assert!(!responses.data.is_empty() || responses.default.is_some(), "OpenAPI operation must declare at least one response");
                    responses
                },
            }
        }
    };
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(meta: syn::Meta) -> darling::Result<RouteMeta> {
        let syn::Meta::List(meta) = meta else { unreachable!() };
        RouteMeta::from_list(&NestedMeta::parse_meta_list(meta.tokens)?)
    }

    fn parse_validated(meta: syn::Meta) -> darling::Result<RouteMeta> {
        let meta = parse(meta)?;
        meta.validate()?;
        Ok(meta)
    }

    /// Parse `#[api(...)]` as the macro does: request bodies are split off, the rest is
    /// parsed as `RouteMeta`, and both halves are validated.
    fn parse_api(meta: syn::Meta) -> darling::Result<(RouteMeta, Vec<RequestBodyMeta>)> {
        let syn::Meta::List(meta) = meta else { unreachable!() };
        let (request_bodies, args) = split_request_bodies(NestedMeta::parse_meta_list(meta.tokens)?)?;
        Ok((RouteMeta::from_list(&args)?, request_bodies))
    }

    fn parse_api_validated(meta: syn::Meta) -> darling::Result<(RouteMeta, Vec<RequestBodyMeta>)> {
        let (route, request_bodies) = parse_api(meta)?;
        route.validate()?;
        validate_request_bodies(&request_bodies)?;
        Ok((route, request_bodies))
    }

    #[test]
    fn response_declarations_validate_statuses_and_body_types() {
        let meta = parse(syn::parse_quote!(api(
            responses(
                response(status = 201, body = "Vec<crate::User>", description = "Created"),
                response(status = 204)
            ),
            drop_default
        )))
        .unwrap();
        assert_eq!(meta.responses.unwrap().0.len(), 2);
        assert!(meta.drop_default.is_present());
        for invalid in [
            syn::parse_quote!(api(responses(response(status = 99)))),
            syn::parse_quote!(api(responses(response(status = 600)))),
            syn::parse_quote!(api(responses(response(status = 204, body = "String")))),
            syn::parse_quote!(api(responses(response(status = 200, body = "Vec<")))),
            syn::parse_quote!(api(responses(response(status = 200, content_type = "text/plain")))),
            syn::parse_quote!(api(responses(response(status = 404), response(status = 404)))),
        ] {
            assert!(parse(invalid).is_err());
        }
    }

    #[test]
    fn explicit_errors_require_nonempty_valid_response_declarations() {
        let meta = parse(syn::parse_quote!(api(errors(
            response(status = 404, body = "Problem", description = "Missing"),
            response(status = 409, body = "Problem")
        ))))
        .unwrap();
        assert_eq!(meta.errors.unwrap().0.len(), 2);
        assert!(parse(syn::parse_quote!(api(errors()))).is_err());
        // Both attributes use the same response validation; one invalid case checks the wiring.
        assert!(parse(syn::parse_quote!(api(errors(response(status = 600))))).is_err());
    }

    #[test]
    fn skip_response_requires_a_nonempty_standalone_contract() {
        let meta = parse_validated(syn::parse_quote!(api(skip_response, responses(response(status = 201, body = "User"))))).unwrap();
        assert!(meta.skip_response.is_present());

        for invalid in [
            syn::parse_quote!(api(skip_response)),
            syn::parse_quote!(api(skip_response, responses())),
            syn::parse_quote!(api(skip_response, responses(response(status = 200)), drop_default)),
            syn::parse_quote!(api(skip_response, errors(response(status = 404)), responses(response(status = 200)))),
        ] {
            assert!(parse_validated(invalid).is_err());
        }
    }

    #[test]
    fn parameter_attributes_are_parsed_structurally() {
        let skip: syn::Attribute = syn::parse_quote!(#[api(skip)]);
        assert!(validate_parameter_skip(&skip).is_ok());

        for invalid in [
            syn::parse_quote!(#[api(skipper)]),
            syn::parse_quote!(#[api(skip = true)]),
            syn::parse_quote!(#[api()]),
            syn::parse_quote!(#[api(skip, skip)]),
            syn::parse_quote!(#[api(id = "not_a_parameter")]),
        ] {
            let error = validate_parameter_skip(&invalid).unwrap_err();
            assert!(error.to_string().contains("only #[api(skip)] is supported"));
        }
    }

    #[test]
    fn request_body_declarations_default_content_type_and_requiredness() {
        let (_, bodies) = parse_api(syn::parse_quote!(api(request_body(body = "UploadForm")))).unwrap();
        let declaration = &bodies[0];
        assert_eq!(declaration.content_type, "application/json");
        assert!(declaration.required);
        assert_eq!(declaration.description, None);

        let (_, bodies) = parse_api(syn::parse_quote!(api(request_body(
            content_type = "multipart/form-data",
            body = "UploadForm",
            description = "File upload",
            required = false
        ))))
        .unwrap();
        let declaration = &bodies[0];
        assert_eq!(declaration.content_type, "multipart/form-data");
        assert_eq!(declaration.description.as_deref(), Some("File upload"));
        assert!(!declaration.required);
    }

    #[test]
    fn request_body_declarations_validate_body_types_and_media_types() {
        for invalid in [
            // `body` is mandatory and must name a type.
            syn::parse_quote!(api(request_body(content_type = "text/plain"))),
            syn::parse_quote!(api(request_body(body = "Vec<"))),
            syn::parse_quote!(api(request_body(body = "Upload", unknown = "x"))),
            syn::parse_quote!(api(request_body)),
            syn::parse_quote!(api(request_body = "Upload")),
        ] {
            assert!(parse_api(invalid).is_err());
        }

        for invalid in [
            // Entries describe one body, so media types must be distinct and the shared
            // description and requiredness must agree.
            syn::parse_quote!(api(
                request_body(body = "Upload", content_type = "application/json"),
                request_body(body = "Upload", content_type = "application/json")
            )),
            syn::parse_quote!(api(
                request_body(body = "A", required = true),
                request_body(body = "B", content_type = "text/plain", required = false)
            )),
            syn::parse_quote!(api(
                request_body(body = "A", description = "one"),
                request_body(body = "B", content_type = "text/plain", description = "two")
            )),
        ] {
            assert!(parse_api_validated(invalid).is_err());
        }

        let (_, bodies) = parse_api_validated(syn::parse_quote!(api(
            request_body(body = "A", content_type = "application/json", description = "payload", required = true),
            request_body(body = "B", content_type = "text/plain", description = "payload", required = true)
        )))
        .unwrap();
        assert_eq!(bodies.len(), 2);
    }

    #[test]
    fn item_declarations_require_a_content_type_and_a_content_status() {
        let meta = parse(syn::parse_quote!(api(responses(response(
            status = 200,
            content_type = "text/event-stream",
            item = "Event"
        )))))
        .unwrap();
        let item = meta.responses.unwrap().0[0].item.as_ref().map(|item| quote!(#item).to_string());
        assert_eq!(item.as_deref(), Some("Event"));

        for invalid in [
            syn::parse_quote!(api(responses(response(status = 200, item = "Event")))),
            syn::parse_quote!(api(responses(response(status = 200, item = "Vec<")))),
            syn::parse_quote!(api(responses(response(status = 204, content_type = "text/event-stream", item = "Event")))),
            syn::parse_quote!(api(responses(response(status = 101, content_type = "text/event-stream", item = "Event")))),
        ] {
            assert!(parse_validated(invalid).is_err());
        }
    }
}
