//! The [`ParameterProvider`] trait maps an axum extractor type to the OpenAPI parameters /
//! request body it contributes to an operation.
//!
//! Like [`crate::Responsible`], it lives in this crate (behind the `axum` feature) rather
//! than in `gotcha` because the generic `Path<T>` impl overlaps the tuple impls
//! `Path<(T1,)>` / `Path<(T1, T2)>`. The compiler can only prove those do not overlap — i.e.
//! that `(T1,)` does not implement [`Schematic`] — in the crate where `Schematic` is defined.

use std::collections::BTreeMap;

use axum::extract::{Extension, Json, Path, Query, Request, State};
use either::Either;
use oas::{MediaType, Parameter, ParameterIn, Referenceable, RequestBody, Schema};

use crate::Schematic;

/// ParameterProvider is a trait that defines the value which can be used as a parameter.
pub trait ParameterProvider {
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
        schema: Some(Referenceable::Data(schema)),
        example: None,
        examples: None,
        content: None,
    }
}

impl<T1: Schematic> ParameterProvider for Path<(T1,)> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        let pattern = regex::Regex::new(r":([^/]+)").unwrap();
        let param_names_in_path: Vec<String> = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string()).collect();

        let t1_param = build_param(
            param_names_in_path.first().cloned().expect("cannot get param in path"),
            ParameterIn::Path,
            T1::required(),
            T1::generate_schema().schema,
            T1::doc(),
        );
        Either::Left(vec![t1_param])
    }
}

impl<T1: Schematic, T2: Schematic> ParameterProvider for Path<(T1, T2)> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        let pattern = regex::Regex::new(r":([^/]+)").unwrap();
        let param_names_in_path: Vec<String> = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string()).collect();

        let t1_param = build_param(
            param_names_in_path.first().cloned().expect("cannot get param in path"),
            ParameterIn::Path,
            T1::required(),
            T1::generate_schema().schema,
            T1::doc(),
        );
        let t2_param = build_param(
            param_names_in_path.get(1).cloned().expect("cannot get param in path"),
            ParameterIn::Path,
            T2::required(),
            T2::generate_schema().schema,
            T2::doc(),
        );

        Either::Left(vec![t1_param, t2_param])
    }
}

impl<T: Schematic> ParameterProvider for Path<T> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        let mut ret = vec![];
        let mut schema = T::generate_schema();

        // Check if this is a struct with properties or a simple type
        if let Some(mut properties) = schema.schema.extras.remove("properties") {
            // Case 1: Struct with properties - each property becomes a path parameter
            if let Some(properties) = properties.as_object_mut() {
                properties.iter_mut().for_each(|(key, value)| {
                    let schema = serde_json::from_value(value.clone()).unwrap();
                    let param = build_param(key.to_string(), ParameterIn::Path, T::required(), schema, T::doc());
                    ret.push(param);
                })
            }
        } else {
            // Case 2: Simple type like Uuid - extract parameter name from URL
            let pattern = regex::Regex::new(r":([^/]+)").unwrap();
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
                schema: Some(Referenceable::Data(schema.schema)),
                example: None,
                examples: None,
                encoding: None,
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
