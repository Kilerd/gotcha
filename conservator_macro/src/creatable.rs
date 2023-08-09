use darling::FromDeriveInput;
use itertools::Itertools;
use proc_macro_error::abort;
use quote::quote;
use syn::{parse2, Data, DeriveInput};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(crud))]
struct CreatableOpts {
    ident: syn::Ident,
}

pub(crate) fn handle_creatable(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let creatable_opts: CreatableOpts = CreatableOpts::from_derive_input(&x1).unwrap();

    let ident = creatable_opts.ident;

    if let Data::Struct(ref body) = x1.data {
        let fields = body.fields.iter().map(|it| &it.ident).collect::<Vec<_>>();

        let field_list = fields
            .iter()
            .map(|it| format!("{}", it.as_ref().map(|ident| ident.to_string()).expect("ident not found")))
            .join(",");
        let param_list = fields.iter().enumerate().map(|it| it.0).map(|it| format!("${}", it + 1)).join(",");
        let insert_sql = format!("({}) VALUES ({})", field_list, param_list);

        let bind_list = fields.iter().map(|it| {
            quote! { .bind(self. #it)}
        });

        quote! {
            impl ::conservator::Creatable for #ident {
                fn get_insert_sql(&self) -> &str {
                    #insert_sql
                }
                fn build<'q, O>(
                    self,
                    e: ::sqlx::query::QueryAs<'q, ::sqlx::Postgres, O, <::sqlx::Postgres as ::sqlx::database::HasArguments<'q>>::Arguments,>,
                ) -> ::sqlx::query::QueryAs<'q, ::sqlx::Postgres, O, <::sqlx::Postgres as ::sqlx::database::HasArguments<'q>>::Arguments,> {
                    e
                    #(#bind_list)*
                }
            }
        }
    } else {
        abort! { x1,
            "enum does not support"
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn compile_fail() {
        let t = trybuild::TestCases::new();
        t.compile_fail("tests/fail/*.rs");
    }
}
