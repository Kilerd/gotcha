use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};

mod authorization;
mod auto;
mod creatable;
mod domain;
mod sql;

#[proc_macro_derive(Domain, attributes(domain))]
#[proc_macro_error]
pub fn derive_domain_fn(input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    match domain::handler(stream2) {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err((span, msg)) => abort! {span, msg},
    }
}

#[proc_macro_derive(Creatable)]
#[proc_macro_error]
pub fn derive_creatable_fn(input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    let stream1 = proc_macro::TokenStream::from(creatable::handle_creatable(stream2));
    stream1
}

#[proc_macro_attribute]
pub fn auto(_args: TokenStream, input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    let stream1 = proc_macro::TokenStream::from(auto::handler(stream2));
    stream1
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn sql(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(args);
    let stream2 = proc_macro2::TokenStream::from(input);
    match sql::handler(args, stream2) {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err((span, msg)) => abort! {span, msg},
    }
}
