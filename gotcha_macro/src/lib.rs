use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};

mod route;
mod schematic;

pub(crate) mod utils;

#[proc_macro_attribute]
pub fn api(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    route::request_handler(args, input_stream)
}

#[proc_macro_derive(Schematic, attributes(schematic))]
#[proc_macro_error]
pub fn derive_parameter(input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    match schematic::handler(stream2) {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err((span, msg)) => abort! {span, msg},
    }
}
