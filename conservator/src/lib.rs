use async_trait::async_trait;
pub use conservator_macro::{auto, sql, Creatable, Domain};
// pub use conservator_macro::authorization;

// #[macro_export]
// macro_rules! auto {
//     () => {
//         todo!("macro can not be used outside magic proc macro")
//     };
// }

// #[macro_export]
// macro_rules! sql {
//     ($value: expr) => {
//         todo!("macro can not be used outside magic proc macro")
//     };
// }

#[async_trait]
pub trait Domain: Sized {
    const PK_FIELD_NAME: &'static str;
    const TABLE_NAME: &'static str;

    type PrimaryKey;

    async fn find_by_pk<'e, 'c: 'e, E: 'e + sqlx::Executor<'c, Database = sqlx::Postgres>>(
        pk: &Self::PrimaryKey, executor: E,
    ) -> Result<Option<Self>, sqlx::Error>;

    async fn fetch_one_by_pk<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>>(
        pk: &Self::PrimaryKey, executor: E,
    ) -> Result<Self, ::sqlx::Error>;

    async fn fetch_all<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>>(executor: E) -> Result<Vec<Self>, ::sqlx::Error>;

    async fn create<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>, C: Creatable>(data: C, executor: E) -> Result<Self, ::sqlx::Error>;

    async fn delete_by_pk<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>>(
        pk: &Self::PrimaryKey, executor: E,
    ) -> Result<(), ::sqlx::Error>;

    async fn update<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>>(entity: Self, executor: E) -> Result<Self, ::sqlx::Error>;
}

pub trait Creatable: Send {
    fn get_insert_sql(&self) -> &str;
    fn build<'q, O>(
        self, e: ::sqlx::query::QueryAs<'q, ::sqlx::Postgres, O, <::sqlx::Postgres as ::sqlx::database::HasArguments<'q>>::Arguments>,
    ) -> ::sqlx::query::QueryAs<'q, ::sqlx::Postgres, O, <::sqlx::Postgres as ::sqlx::database::HasArguments<'q>>::Arguments>;
}
