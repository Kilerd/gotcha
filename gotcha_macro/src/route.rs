use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, FnArg, ItemFn, ReturnType};

use darling::util::Flag;

use crate::utils::AttributesExt;
use crate::FromMeta;

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
}

#[derive(Debug, Default)]
struct ResponseDeclarations(Vec<ResponseMeta>);

#[derive(Debug)]
struct ErrorDeclarations(Vec<ResponseMeta>);

impl FromMeta for ErrorDeclarations {
    fn from_list(items: &[syn::NestedMeta]) -> darling::Result<Self> {
        let ResponseDeclarations(responses) = ResponseDeclarations::from_list(items)?;
        if responses.is_empty() {
            return Err(darling::Error::custom("errors(...) must declare at least one response"));
        }
        Ok(Self(responses))
    }
}

#[derive(Debug, FromMeta)]
struct ResponseMeta {
    status: u16,
    body: Option<String>,
    content_type: Option<String>,
    description: Option<String>,
}

impl FromMeta for ResponseDeclarations {
    fn from_list(items: &[syn::NestedMeta]) -> darling::Result<Self> {
        let mut responses = Vec::new();
        let mut statuses = std::collections::BTreeSet::new();
        for item in items {
            let syn::NestedMeta::Meta(syn::Meta::List(meta)) = item else {
                return Err(darling::Error::custom("expected response(status = ..., body = \"Type\")").with_span(item));
            };
            if !meta.path.is_ident("response") {
                return Err(darling::Error::custom("expected response(...)").with_span(meta));
            }
            let response = ResponseMeta::from_list(&meta.nested.iter().cloned().collect::<Vec<_>>())?;
            if !(100..=599).contains(&response.status) {
                return Err(darling::Error::custom("response status must be in 100..=599").with_span(meta));
            }
            if !statuses.insert(response.status) {
                return Err(darling::Error::custom("duplicate response status").with_span(meta));
            }
            if let Some(body) = &response.body {
                syn::parse_str::<syn::Type>(body).map_err(|e| darling::Error::custom(e).with_span(meta))?;
                if response.status < 200 || matches!(response.status, 204 | 205 | 304) {
                    return Err(darling::Error::custom("this status cannot declare a response body").with_span(meta));
                }
            } else if response.content_type.is_some() {
                return Err(darling::Error::custom("content_type requires a body type").with_span(meta));
            }
            responses.push(response);
        }
        Ok(Self(responses))
    }
}

impl ResponseMeta {
    fn tokens(&self) -> proc_macro2::TokenStream {
        let status = self.status;
        let description = self.description.clone().unwrap_or_else(|| format!("HTTP {status}"));
        match &self.body {
            Some(body) => {
                let body: syn::Type = syn::parse_str(body).expect("validated body type");
                let media_type = self.content_type.as_deref().unwrap_or("application/json");
                quote!(::gotcha::response::response::<#body>(#status, #media_type, #description))
            }
            None => quote!(::gotcha::response::empty_response(#status, #description)),
        }
    }
}

pub(crate) fn request_handler(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);

    let args = match RouteMeta::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let meta = args;
    let extra_responses: Vec<_> = meta.responses.unwrap_or_default().0.iter().map(ResponseMeta::tokens).collect();
    let explicit_errors = meta.errors.is_some();
    let error_responses: Vec<_> = meta.errors.into_iter().flat_map(|errors| errors.0).map(|response| response.tokens()).collect();
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
    let mut input = parse_macro_input!(input_stream as ItemFn);

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

    let params_token: Vec<proc_macro2::TokenStream> = input
        .sig
        .inputs
        .iter()
        .flat_map(|param| match param {
            FnArg::Receiver(_) => None,
            FnArg::Typed(typed) => {
                // Check if the parameter has the #[api(skip)] attribute
                let should_skip = typed.attrs.iter().any(|attr| {
                    if attr.path.is_ident("api") {
                        // Parse the attribute tokens to check for "skip"
                        let tokens = attr.tokens.to_string();
                        tokens.contains("skip")
                    } else {
                        false
                    }
                });

                if should_skip {
                    return None;
                }
                let ty = &typed.ty;
                Some(quote! { <#ty as ::gotcha::ParameterProvider>::generate })
            }
        })
        .collect();
    let ret_type = match &input.sig.output {
        // A handler without an explicit return type returns `()`.
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let ret_responses = if explicit_errors {
        quote!(<#ret_type as ::gotcha::response::ResultResponse>::success_responses())
    } else {
        quote!(<#ret_type as ::gotcha::Responsible>::response())
    };

    input.sig.inputs.iter_mut().for_each(|param| {
        if let FnArg::Typed(typed) = param {
            // Remove only the #[api(...)] attributes, keep others
            typed.attrs.retain(|attr| !attr.path.is_ident("api"));
        }
    });

    let ret = quote! {

        #input

        ::gotcha::inventory::submit! {
            ::gotcha::Operable {
                type_name: concat!(module_path!(), "::", #fn_ident_string),
                id: #operation_id,
                group: #group,
                summary: #summary,
                description: #docs,
                deprecated: #deprecated,
                security: #security,
                parameters: &[#(#params_token,)*],
                responses: || {
                    let mut responses = #ret_responses;
                    // Declared Err alternatives join the inferred Ok contract, including shared statuses.
                    #(::gotcha::response::merge_responses(&mut responses, #error_responses);)*
                    // Explicit declarations replace inference for their status only.
                    #(responses.data.extend((#extra_responses).data);)*
                    if #drop_default { responses.default = None; }
                    assert!(!responses.data.is_empty() || responses.default.is_some(), "OpenAPI operation must declare at least one response");
                    responses
                },
            }
        }
    };
    TokenStream::from(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(meta: syn::Meta) -> darling::Result<RouteMeta> {
        let syn::Meta::List(meta) = meta else { unreachable!() };
        RouteMeta::from_list(&meta.nested.into_iter().collect::<Vec<_>>())
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
}
