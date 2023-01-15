use async_trait::async_trait;

pub use conservator_macro::magic;
pub use conservator_macro::Creatable;
pub use conservator_macro::Crud;
// pub use conservator_macro::authorization;

#[async_trait]
pub trait Crud: Sized {
    type PrimaryKey;

    fn table_name() -> &'static str;

    async fn find_by_id<'e, 'c: 'e, E: 'e + sqlx::Executor<'c, Database = sqlx::Postgres>>(
        pk: &Self::PrimaryKey,
        executor: E,
    ) -> Result<Option<Self>, sqlx::Error>;

    async fn fetch_one_by_id<
        'e,
        'c: 'e,
        E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>,
    >(
        pk: &Self::PrimaryKey,
        executor: E,
    ) -> Result<Self, ::sqlx::Error>;

    async fn fetch_all<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>>(
        executor: E,
    ) -> Result<Vec<Self>, ::sqlx::Error>;

    async fn create<
        'e,
        'c: 'e,
        E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>,
        C: Creatable,
    >(
        data: C,
        executor: E,
    ) -> Result<Self, ::sqlx::Error>;
}

pub trait Creatable: Send {
    fn get_insert_sql(&self) -> &str;
    fn build<'q, O>(
        self,
        e: ::sqlx::query::QueryAs<
            'q,
            ::sqlx::Postgres,
            O,
            <::sqlx::Postgres as ::sqlx::database::HasArguments<'q>>::Arguments,
        >,
    ) -> ::sqlx::query::QueryAs<
        'q,
        ::sqlx::Postgres,
        O,
        <::sqlx::Postgres as ::sqlx::database::HasArguments<'q>>::Arguments,
    >;
}
