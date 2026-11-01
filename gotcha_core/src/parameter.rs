//! The [`ParameterProvider`] trait maps an axum extractor type to the OpenAPI parameters /
//! request body it contributes to an operation.
//!
//! It lives in this crate (behind the `axum` feature) rather
//! than in `gotcha` because the generic `Path<T>` impl overlaps the tuple impls
//! `Path<(T1,)>` through `Path<(T1, ..., T16)>`. The compiler can only prove those do not
//! overlap — i.e. that a tuple does not implement [`Schematic`] — in the crate where
//! `Schematic` is defined.

use std::any::TypeId;
use std::collections::BTreeMap;

use axum::extract::{Extension, Form, Json, Path, Query, Request, State};
use axum::http::StatusCode;
use either::Either;
use oas::{MediaType, Parameter, ParameterIn, RequestBody, Schema};

use crate::Schematic;

/// A Gotcha extractor whose rejections are rendered by a per-provider configuration.
///
/// [`ParameterProvider::extractor_kinds`] reports which of these an argument uses, so document
/// assembly can add the rejection responses the application is configured to send. Axum's own
/// extractors report none: their rejections keep Axum's plain-text response.
///
/// A provider is named by [`Provider`](Self::Provider), built with `gotcha::extract::kind::<P>()`
/// for any `gotcha::extract::RejectionProvider`. The deprecated unit variants name Gotcha's
/// built-in providers, which still report them, and document exactly like their `kind::<P>()`; a
/// unit variant and the matching `Provider` key are still different values.
///
/// This enum is `#[non_exhaustive]`: a later release may add a provider, so a `match` outside this
/// crate needs a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ExtractorKind {
    /// The JSON body provider. Rejections are configured with `JsonConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::JsonProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Json,
    /// The URL-encoded form body provider. Rejections are configured with `FormConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::FormProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Form,
    /// The query-string provider. Rejections are configured with `QueryConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::QueryProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Query,
    /// The path-parameter provider. Rejections are configured with `PathConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::PathProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Path,
    /// The multipart form provider. Rejections are configured with `MultipartConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::MultipartProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Multipart,
    /// The typed-header provider. Rejections are configured with `TypedHeaderConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::TypedHeaderProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    TypedHeader,
    /// Gotcha's typed header parameter. Rejections are configured with `HeaderConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::HeaderProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Header,
    /// Gotcha's typed cookie parameter. Rejections are configured with `CookieConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::CookieProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Cookie,
    /// The `Valid` wrapper's validation failure. Configured with `ValidConfig`.
    #[deprecated(
        note = "name the provider with `gotcha::extract::kind::<gotcha::extract::ValidProvider>()`; Gotcha's own extractors report this variant until the next major release"
    )]
    Valid,
    /// A rejection provider identified by its type. Built by `gotcha::extract::kind::<P>()`.
    Provider(ProviderKey),
    /// Every kind another extractor reports, built with [`ExtractorKind::all`].
    All(KindList),
}

impl ExtractorKind {
    /// Every kind `kinds` reports, resolved when a document is generated.
    ///
    /// A wrapper extractor uses this to document the rejections of the extractor it wraps:
    /// `ExtractorKind::all(<E as ParameterProvider>::extractor_kinds)`.
    pub const fn all(kinds: fn() -> &'static [ExtractorKind]) -> Self {
        ExtractorKind::All(KindList(kinds))
    }
}

/// The identity of a rejection provider and the statuses it documents by default.
///
/// Only `gotcha::extract::kind` builds one, so the default statuses always come from the provider
/// itself. Keys compare, order, and hash by the provider's [`TypeId`]; their order is consistent
/// within one build but can change between builds.
#[derive(Clone, Copy)]
pub struct ProviderKey {
    id: fn() -> TypeId,
    name: fn() -> &'static str,
    statuses: fn() -> &'static [(StatusCode, &'static str)],
}

impl ProviderKey {
    #[doc(hidden)]
    pub const fn from_parts(id: fn() -> TypeId, name: fn() -> &'static str, statuses: fn() -> &'static [(StatusCode, &'static str)]) -> Self {
        Self { id, name, statuses }
    }

    /// The provider's type id.
    #[doc(hidden)]
    pub fn id(&self) -> TypeId {
        (self.id)()
    }

    /// The provider's type name, for diagnostics.
    pub fn name(&self) -> &'static str {
        (self.name)()
    }

    /// The statuses the provider documents when the application declares none, each with the
    /// phrase that describes it.
    #[doc(hidden)]
    pub fn default_statuses(&self) -> &'static [(StatusCode, &'static str)] {
        (self.statuses)()
    }
}

impl std::fmt::Debug for ProviderKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

impl PartialEq for ProviderKey {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for ProviderKey {}

impl PartialOrd for ProviderKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ProviderKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(&other.id())
    }
}

impl std::hash::Hash for ProviderKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state)
    }
}

/// Another extractor's kinds, expanded when a document is generated. See [`ExtractorKind::all`].
///
/// Lists compare, order, and hash by the kinds they expand to.
#[derive(Clone, Copy)]
pub struct KindList(fn() -> &'static [ExtractorKind]);

impl KindList {
    /// The kinds this list stands for.
    #[doc(hidden)]
    pub fn kinds(&self) -> &'static [ExtractorKind] {
        (self.0)()
    }
}

impl std::fmt::Debug for KindList {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_list().entries(self.kinds()).finish()
    }
}

impl PartialEq for KindList {
    fn eq(&self, other: &Self) -> bool {
        self.kinds() == other.kinds()
    }
}

impl Eq for KindList {}

impl PartialOrd for KindList {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for KindList {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.kinds().cmp(other.kinds())
    }
}

impl std::hash::Hash for KindList {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kinds().hash(state)
    }
}

/// ParameterProvider is a trait that defines the value which can be used as a parameter.
pub trait ParameterProvider {
    /// What this extractor contributes to the operation, given the route path.
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        Either::Left(vec![])
    }

    /// Whether this extractor consumes the whole request body.
    ///
    /// `#[api(request_body(...))]` uses this to drop the body such an extractor would otherwise
    /// contribute, while keeping the path, query, header, and cookie parameters of the others.
    /// Implement it as `true` for custom whole-body extractors; the default is `false`. Leaving
    /// the default still lets a declaration replace the body and win, but the replaced type stays
    /// registered as a component.
    fn is_request_body() -> bool {
        false
    }

    /// The Gotcha providers whose rejection responses this extractor can produce.
    ///
    /// Document generation adds one response per status each provider rejects with, using the
    /// application's configured error handler and body type. The default is empty, so Axum's own
    /// extractors and user-defined implementations keep their own rejection contract and document
    /// nothing automatically.
    fn extractor_kinds() -> &'static [ExtractorKind] {
        &[]
    }
}

fn build_param(name: String, _in: ParameterIn, required: bool, schema: Schema, description: Option<String>) -> Parameter {
    Parameter {
        name,
        _in,
        description,
        required: Some(required),
        deprecated: None,
        allow_empty_value: None,
        style: None,
        explode: None,
        allow_reserved: None,
        schema: Some(schema.into()),
        example: None,
        examples: None,
        content: None,
    }
}

/// Implement [`ParameterProvider`] for [`Path`] tuples of a given arity.
///
/// axum deserialises `Path<T>` through serde, which implements `Deserialize` for tuples up to
/// 16 elements, so 16 is the largest route arity this can document. Parameters are named and
/// ordered by the route's `{name}` segments.
macro_rules! impl_path_tuple {
    ($($t: ident),+) => {
        impl<$($t: Schematic),+> ParameterProvider for Path<($($t,)+)> {
            fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
                // Since axum 0.8 a captured segment is written `{name}`, matching OpenAPI's own syntax.
                let pattern = regex::Regex::new(r"\{([^}]+)\}").unwrap();
                let param_names_in_path: Vec<String> = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string()).collect();
                let found = param_names_in_path.len();
                let expected = [$(stringify!($t)),+].len();
                let types = [$(stringify!($t)),+].join(", ");
                let mut param_names_in_path = param_names_in_path.into_iter();
                let mut params = vec![];
                $(
                    let name = param_names_in_path.next().unwrap_or_else(|| {
                        panic!("route '{url}' captures {found} path parameter(s), but Path<({types})> expects {expected}")
                    });
                    // A matched route always supplies its segments, so every path parameter is
                    // required regardless of the element type: OpenAPI mandates `required: true`
                    // for `in: path`, and an `Option` element does not make a segment optional.
                    params.push(build_param(name, ParameterIn::Path, true, $t::generate_schema().schema, $t::doc()));
                )+
                Either::Left(params)
            }
        }
    };
}

impl_path_tuple!(T1);
impl_path_tuple!(T1, T2);
impl_path_tuple!(T1, T2, T3);
impl_path_tuple!(T1, T2, T3, T4);
impl_path_tuple!(T1, T2, T3, T4, T5);
impl_path_tuple!(T1, T2, T3, T4, T5, T6);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
impl_path_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);

impl<T: Schematic> ParameterProvider for Path<T> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        // Path parameters are always required: OpenAPI mandates `required: true` for `in: path`,
        // and a matched route always supplies its segments. Neither a field's own requiredness
        // nor `T`'s applies here.
        // Case 1: a struct extractor — each field becomes one path parameter. Read the fields
        // directly (as `Query` does) instead of scraping `properties` out of the generated
        // schema: during spec assembly a collection scope is active, so a derived struct's
        // `generate_schema()` returns a bare `$ref` carrying no properties at all.
        let fields = T::fields();
        if !fields.is_empty() {
            return Either::Left(
                fields
                    .into_iter()
                    .map(|(name, schema)| {
                        let desc = schema.schema.description.clone();
                        build_param(name.to_string(), ParameterIn::Path, true, schema.schema, desc)
                    })
                    .collect(),
            );
        }

        let mut ret = vec![];
        let mut schema = T::generate_schema();

        // Check if this is a hand-written impl exposing properties or a simple type
        if let Some(mut properties) = schema.schema.extras.remove("properties") {
            // Case 2: object schema with properties - each property becomes a path parameter
            if let Some(properties) = properties.as_object_mut() {
                properties.iter_mut().for_each(|(key, value)| {
                    let schema = serde_json::from_value(value.clone()).unwrap();
                    let param = build_param(key.to_string(), ParameterIn::Path, true, schema, T::doc());
                    ret.push(param);
                })
            }
        } else {
            // Case 3: Simple type like Uuid - extract parameter name from URL
            // Since axum 0.8 a captured segment is written `{name}`, matching OpenAPI's own syntax.
            let pattern = regex::Regex::new(r"\{([^}]+)\}").unwrap();
            let param_names_in_path: Vec<String> = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string()).collect();

            if let Some(param_name) = param_names_in_path.first() {
                let param = build_param(param_name.clone(), ParameterIn::Path, true, T::generate_schema().schema, T::doc());
                ret.push(param);
            }
        }
        Either::Left(ret)
    }
}

impl<T: Schematic> ParameterProvider for Json<T> {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        let mut contents = BTreeMap::new();

        let schema = T::generate_schema();
        contents.insert(
            "application/json".to_owned(),
            MediaType {
                schema: Some(schema.schema.into()),
                example: None,
                examples: None,
                encoding: None,
                item_schema: None,
            },
        );
        let req_body = RequestBody {
            description: T::doc(),
            required: Some(T::required()),
            content: contents,
        };
        Either::Right(req_body)
    }

    fn is_request_body() -> bool {
        true
    }
}

impl<T: Schematic> ParameterProvider for Form<T> {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        let mut contents = BTreeMap::new();

        let schema = T::generate_schema();
        contents.insert(
            "application/x-www-form-urlencoded".to_owned(),
            MediaType {
                schema: Some(schema.schema.into()),
                example: None,
                examples: None,
                encoding: None,
                item_schema: None,
            },
        );
        let req_body = RequestBody {
            description: T::doc(),
            required: Some(T::required()),
            content: contents,
        };
        Either::Right(req_body)
    }

    fn is_request_body() -> bool {
        true
    }
}

impl<T: Schematic> ParameterProvider for Query<T> {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        // A non-required `Query<T>` (for example `Query<Option<S>>`) has no parameters when the
        // value is absent, so every field it documents is optional.
        let mut fields = T::fields();
        if !T::required() {
            for (_, schema) in &mut fields {
                schema.required = false;
            }
        }
        if fields.is_empty() {
            let mut ret = vec![];

            let mut schema = T::generate_schema();

            if let Some(mut properties) = schema.schema.extras.remove("properties") {
                if let Some(properties) = properties.as_object_mut() {
                    properties.iter_mut().for_each(|(key, value)| {
                        let schema = serde_json::from_value(value.clone()).unwrap();
                        let param = build_param(key.to_string(), ParameterIn::Query, T::required(), schema, T::doc());
                        ret.push(param);
                    })
                }
            }
            Either::Left(ret)
        } else {
            Either::Left(
                fields
                    .into_iter()
                    .map(|(name, schema)| {
                        let desc = schema.schema.description.clone();
                        build_param(name.to_string(), ParameterIn::Query, schema.required, schema.schema, desc)
                    })
                    .collect(),
            )
        }
    }
}

impl<T> ParameterProvider for State<T> {}

impl<T> ParameterProvider for Extension<T> {}

/// The whole header map is untyped, so there is nothing to document — but a handler must still be
/// able to take it. Individual headers are documented with `gotcha::Header<T>`.
impl ParameterProvider for axum::http::HeaderMap {}

/// axum's own typed headers document themselves: [`axum_extra::headers::Header`] already carries
/// the header name, so `TypedHeader<UserAgent>` needs no extra declaration. Header values are text
/// on the wire, so the schema is a string.
impl<T: axum_extra::headers::Header> ParameterProvider for axum_extra::TypedHeader<T> {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        let schema = Schema {
            _type: Some("string".into()),
            format: None,
            nullable: None,
            description: None,
            extras: Default::default(),
            ..Schema::default()
        };
        Either::Left(vec![build_param(T::name().as_str().to_string(), ParameterIn::Header, true, schema, None)])
    }
}

/// axum treats `Option<E>` as "extract `E`, or `None` if it isn't there", so a request body `E`
/// contributes becomes optional and its non-path parameters are no longer required. Path
/// parameters stay required: OpenAPI mandates `required: true` for `in: path`, and a matched
/// route always supplies its segments.
impl<T: ParameterProvider> ParameterProvider for Option<T> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        match T::generate(url) {
            Either::Left(params) => Either::Left(
                params
                    .into_iter()
                    .map(|mut param| {
                        if !matches!(&param._in, ParameterIn::Path) {
                            param.required = Some(false);
                        }
                        param
                    })
                    .collect(),
            ),
            Either::Right(mut body) => {
                body.required = Some(false);
                Either::Right(body)
            }
        }
    }

    fn is_request_body() -> bool {
        T::is_request_body()
    }

    fn extractor_kinds() -> &'static [ExtractorKind] {
        T::extractor_kinds()
    }
}

impl ParameterProvider for Request {}

impl ParameterProvider for axum::extract::multipart::Multipart {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        let mut contents = BTreeMap::new();

        contents.insert(
            "multipart/form-data".to_owned(),
            MediaType {
                schema: None,
                example: None,
                examples: None,
                encoding: None,
                item_schema: None,
            },
        );

        let req_body = RequestBody {
            description: Some("Multipart form data".to_owned()),
            required: Some(true),
            content: contents,
        };

        Either::Right(req_body)
    }

    fn is_request_body() -> bool {
        true
    }
}

/// The WebSocket upgrade handshake contributes no parameter of its own; a route that takes it
/// declares its `101` response with `#[api(responses(...))]`.
impl ParameterProvider for axum::extract::ws::WebSocketUpgrade {}
