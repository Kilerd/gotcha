use quote::quote;

use itertools::Itertools;
use proc_macro2::Span;

use quote::format_ident;
use std::str::FromStr;
use strum::EnumString;
use syn::{parse2, spanned::Spanned, Expr, ExprLit, FnArg, ItemFn, Lit, LitStr, Pat, Stmt};

#[derive(Debug, EnumString)]
#[strum(serialize_all = "snake_case")]
enum Action {
    Fetch,
    Exists,
    Find,
    FetchAll,
}

impl Action {
    fn build_sqlx_query(&self, fields: &[String], sql: String) -> proc_macro2::TokenStream {
        let fields = fields
            .iter()
            .filter(|&field| !field.eq("executor"))
            .map(|field| format_ident!("{}", field))
            .collect_vec();
        match self {
            Action::Fetch => {
                quote! {
                    ::sqlx::query_as(#sql)
                    #(.bind(#fields))*
                    .fetch_one(executor)
                    .await
                }
            }
            Action::Exists => {
                quote! {
                    Ok(::sqlx::query_as::<_, (bool, )>(#sql)
                    #(.bind(#fields))*
                    .fetch_one(executor)
                    .await?.0)
                }
            }
            Action::Find => {
                quote! {
                    ::sqlx::query_as(#sql)
                    #(.bind(#fields))*
                    .fetch_optional(executor)
                    .await
                }
            }
            Action::FetchAll => {
                quote! {
                    ::sqlx::query_as(#sql)
                    #(.bind(#fields))*
                    .fetch_all(executor)
                    .await
                }
            }
        }
    }
}

pub(crate) fn handler(
    args: proc_macro2::TokenStream,
    input: proc_macro2::TokenStream,
) -> Result<proc_macro2::TokenStream, (Span, &'static str)> {
    let arg = args.to_string();
    let action = match Action::from_str(&arg) {
        Ok(action) => action,
        Err(e) => return Err((args.span(), "unknown action type")),
    };

    let input_span = input.span().clone();
    let method = match parse2::<ItemFn>(input) {
        Ok(func) => func,
        Err(e) => return Err((input_span, "unknown action type")),
    };

    let vis = &method.vis;
    let ident = &method.sig.ident;
    let inputs = &method.sig.inputs;
    let fields: Vec<String> = inputs
        .iter()
        .filter_map(|it| match it {
            FnArg::Receiver(_) => None,
            FnArg::Typed(typed) => match &*typed.pat {
                Pat::Ident(ident) => Some(ident.ident.to_string()),
                _ => None,
            },
        })
        .collect();
    let output = &method.sig.output;
    let body = &method.block;
    let body: Vec<proc_macro2::TokenStream> = body
        .stmts
        .iter()
        .cloned()
        .map(|stmt| match &stmt {
            Stmt::Expr(Expr::Lit(expr_lit)) => match &expr_lit.lit {
                Lit::Str(lit_str) => {
                    let mut sql = lit_str.value();
                    fields.iter().enumerate().for_each(|(idx, field)| {
                        sql = sql.replace(&format!(":{}", field), &format!("${}", idx + 1));
                    });
                    let query_stmt = action.build_sqlx_query(&fields[..], sql);
                    quote!( #query_stmt)
                }
                _ => {
                    quote!( #stmt )
                }
            },
            _ => quote!( #stmt ),
        })
        .collect();

    Ok(quote! {
        #vis async fn #ident<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database=::sqlx::Postgres>>(#inputs) #output {
            #(#body )*
        }
    })
}

#[cfg(test)]
mod test {
    use crate::sql::handler;

    #[test]
    fn should_generate_fetch_sql_function() {
        use quote::quote;
        let args = quote! { fetch };
        let input = quote! {
            pub async fn find_user<E>(email: &str, executor: E) -> Result<Option<UserEntity>, ::sqlx::Error> {
                "select * from users where email = :email"
            }
        };

        let expected = quote! {
            pub async fn find_user<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>>(
                email: &str,
                executor: E
            ) -> Result<Option<UserEntity>, ::sqlx::Error> {
                ::sqlx::query_as("select * from users where email = $1")
                    .bind(email)
                    .fetch_one(executor)
                    .await
            }
        };
        assert_eq!(
            expected.to_string(),
            handler(args, input).unwrap().to_string()
        );
    }
}
