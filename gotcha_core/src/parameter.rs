//! The [`ParameterProvider`] trait maps an axum extractor type to the OpenAPI parameters /
//! request body it contributes to an operation.
//!
//! It lives in this crate (behind the `axum` feature) rather
//! than in `gotcha` because the generic `Path<T>` impl overlaps the tuple impls
//! `Path<(T1,)>` through `Path<(T1, ..., T16)>`. The compiler can only prove those do not
//! overlap — i.e. that a tuple does not implement [`Schematic`] — in the crate where
//! `Schematic` is defined.

use std::collections::BTreeMap;

use axum::extract::{Extension, Json, Path, Query, Request, State};
use either::Either;
use oas::{MediaType, Parameter, ParameterIn, RequestBody, Schema};

use crate::Schematic;

/// ParameterProvider is a trait that defines the value which can be used as a parameter.
pub trait ParameterProvider {
    /// What this extractor contributes to the operation, given the route path.
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        Either::Left(vec![])
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
                let mut param_names_in_path = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string());
                let mut params = vec![];
                $(
                    params.push(build_param(
                        param_names_in_path.next().expect("cannot get param in path"),
                        ParameterIn::Path,
                        $t::required(),
                        $t::generate_schema().schema,
                        $t::doc(),
                    ));
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
                        build_param(name.to_string(), ParameterIn::Path, schema.required, schema.schema, desc)
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
                    let param = build_param(key.to_string(), ParameterIn::Path, T::required(), schema, T::doc());
                    ret.push(param);
                })
            }
        } else {
            // Case 3: Simple type like Uuid - extract parameter name from URL
            // Since axum 0.8 a captured segment is written `{name}`, matching OpenAPI's own syntax.
            let pattern = regex::Regex::new(r"\{([^}]+)\}").unwrap();
            let param_names_in_path: Vec<String> = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string()).collect();

            if let Some(param_name) = param_names_in_path.first() {
                let param = build_param(param_name.clone(), ParameterIn::Path, T::required(), T::generate_schema().schema, T::doc());
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
}

impl<T: Schematic> ParameterProvider for Query<T> {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        let fields = T::fields();
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

/// axum treats `Option<E>` as "extract `E`, or `None` if it isn't there", so the parameters `E`
/// contributes are documented but no longer required.
impl<T: ParameterProvider> ParameterProvider for Option<T> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        match T::generate(url) {
            Either::Left(params) => Either::Left(
                params
                    .into_iter()
                    .map(|mut param| {
                        param.required = Some(false);
                        param
                    })
                    .collect(),
            ),
            body => body,
        }
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
}
