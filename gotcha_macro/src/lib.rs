use darling::FromMeta;
use proc_macro::TokenStream;

use route::HttpMethod;
use proc_macro_error::{proc_macro_error, abort};

mod route;
mod schematic;

macro_rules! handler {
    ($name:tt, $method: expr) => {
        #[proc_macro_attribute]
        pub fn $name(args: TokenStream, input_stream: TokenStream) -> TokenStream {
            route::request_handler($method, args, input_stream)
        }
    }
}
handler!(get, HttpMethod::Get);
handler!(post, HttpMethod::Post);
handler!(put, HttpMethod::Put);
handler!(patch, HttpMethod::Patch);
handler!(delete, HttpMethod::Delete);
handler!(options, HttpMethod::Options);
handler!(connect, HttpMethod::Connect);
handler!(head, HttpMethod::Head);
handler!(trace, HttpMethod::Trace);



#[proc_macro_derive(Schematic, attributes(schematic))]
#[proc_macro_error]
pub fn derive_parameter(input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    match schematic::handler(stream2) {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err((span, msg)) => abort! {span, msg}
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