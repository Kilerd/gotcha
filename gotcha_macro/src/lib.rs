use darling::FromMeta;
use proc_macro::TokenStream;
use quote::__private::ext::RepToTokensExt;
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, ItemFn, LitStr, Meta, Lit};

enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
    Connect,
    Trace,
}

impl HttpMethod {
    fn to_token_stream(&self) -> proc_macro2::TokenStream {
        match self {
            HttpMethod::Get => { quote! { ::actix_web::http::Method::GET } }
            HttpMethod::Post => { quote! { ::actix_web::http::Method::POST } }
            HttpMethod::Put => { quote! { ::actix_web::http::Method::PUT } }
            HttpMethod::Patch => { quote! { ::actix_web::http::Method::PATCH } }
            HttpMethod::Delete => { quote! { ::actix_web::http::Method::DELETE } }
            HttpMethod::Options => { quote! { ::actix_web::http::Method::OPTIONS } }
            HttpMethod::Head => { quote! { ::actix_web::http::Method::HEAD } }
            HttpMethod::Connect => { quote! { ::actix_web::http::Method::CONNECT } }
            HttpMethod::Trace => { quote! { ::actix_web::http::Method::Trace } }
        }
    }
}

#[proc_macro_attribute]
pub fn get(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Get, args, input_stream)
}

#[proc_macro_attribute]
pub fn post(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Post, args, input_stream)
}

#[proc_macro_attribute]
pub fn put(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Put, args, input_stream)
}

#[proc_macro_attribute]
pub fn patch(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Patch, args, input_stream)
}

#[proc_macro_attribute]
pub fn delete(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Delete, args, input_stream)
}

#[proc_macro_attribute]
pub fn options(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Options, args, input_stream)
}

#[proc_macro_attribute]
pub fn connect(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Connect, args, input_stream)
}

#[proc_macro_attribute]
pub fn head(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Head, args, input_stream)
}

#[proc_macro_attribute]
pub fn trace(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    request_handler(HttpMethod::Trace, args, input_stream)
}

fn request_handler(method: HttpMethod, args: TokenStream, input_stream: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);

    let args = match RouteMeta::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let RouteMeta { path, extra } = dbg!(args);
    let group = if let Some(group_name) = extra.group {
        dbg! {quote! { Some(#group_name.to_string()) }}
    } else {
        quote! { None }
    };
    let method = method.to_token_stream();

    let input = dbg!(parse_macro_input!(input_stream as ItemFn));
    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();
    let docs: Vec<String> = input.attrs.iter().filter_map(|attr| {
        match attr.parse_meta().unwrap() {
            Meta::NameValue(doc) => { if doc.path.is_ident("doc") { Some(doc) } else { None } }
            _ => None
        }
    }).filter_map(|attr| match attr.lit {
        Lit::Str(lit_str) => Some(lit_str.value()),
        _ => {
            None
        }
    }).map(|doc| doc.trim().to_string()).collect();

    let docs = if docs.is_empty() { quote!(None)} else {
        let t = docs.join("\n");
        quote!{ Some(#t) }
    };

    let ret = quote! {
        #[::actix_web::get( "/" )]
        #input

        impl ::gotcha::Operable for  #fn_ident {
            fn id(&self) -> &'static str {
                #fn_ident_string
            }
            fn method(&self) -> ::actix_web::http::Method {
                #method
            }
            fn uri(&self) -> &'static str {
                #path
            }
            fn group(&self) -> Option<String> {
                #group
            }
            fn description(&self) -> Option<&'static str> {
                #docs
            }
            fn deprecated(&self) -> bool {
                false
            }
        }
    };
    TokenStream::from(ret)
}


#[derive(Debug)]
struct RouteMeta {
    path: LitStr,
    extra: RouteExtraMeta,
}

#[derive(Debug, FromMeta)]
struct RouteExtraMeta {
    group: Option<String>,
}

impl FromMeta for RouteMeta {
    fn from_list(items: &[syn::NestedMeta]) -> darling::Result<Self> {
        if items.len() == 0 {
            panic!("path must be set");
        }
        if !matches!(items[0], syn::NestedMeta::Lit(..)) {
            panic!("first param must be literal");
        }
        let path = match &items[0] {
            syn::NestedMeta::Lit(literal) => match literal {
                syn::Lit::Str(token) => token.clone(),
                _ => return Err(darling::Error::unexpected_type("other literal")),
            },
            _ => return Err(darling::Error::unexpected_type("not literal")),
        };
        let extra_meta = RouteExtraMeta::from_list(&items[1..])?;

        Ok(RouteMeta {
            path,
            extra: extra_meta,
        })
    }
}


#[cfg(test)]
mod test {
    #[test]
    fn pass() {
        let t = trybuild::TestCases::new();
        t.pass("tests/pass/*.rs");
    }
}