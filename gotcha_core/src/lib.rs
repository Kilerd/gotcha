use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;

use axum::extract::{Json, Path, Query, Request, State};
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use either::Either;
use oas::{MediaType, Parameter, ParameterIn, Referenceable, RequestBody, Schema};
pub mod responsable;

pub struct EnhancedSchema {
    pub schema: Schema,
    pub required: bool,
}

/// Schematic is a trait that defines the schema of a type.
pub trait Schematic {
    /// The name of the type.
    fn name() -> &'static str;
    /// Whether the type is required.
    fn required() -> bool;
    /// Whether the type is nullable.
    fn nullable() -> Option<bool> {
        None
    }
    /// The type of the type.
    fn type_() -> &'static str;
    /// The documentation of the type.
    fn doc() -> Option<String> {
        None
    }
    /// The format of the type.
    fn format() -> Option<String> {
        None
    }
    
    fn fields() -> Vec<(&'static str, EnhancedSchema)> {
        vec![]
    }
    /// Generate the schema of the type.
    fn generate_schema() -> EnhancedSchema {
        EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().to_string()),
                format: Self::format(),
                nullable: Self::nullable(),
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        }
    }
}

/// ParameterProvider is a trait that defines the value which can be used as a parameter.
pub trait ParameterProvider {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        Either::Left(vec![])
    }
}

macro_rules! impl_primitive_type {
    ($t: ty, $name: expr, $api_type: expr) => {
        impl Schematic for $t {
            fn name() -> &'static str {
                $name
            }
            fn required() -> bool {
                true
            }
            fn type_() -> &'static str {
                $api_type
            }
        }
    };
}

impl_primitive_type! { i8, "i32", "integer"}
impl_primitive_type! { i16, "i16", "integer"}
impl_primitive_type! { i32, "i32", "integer"}
impl_primitive_type! { i64, "i64", "integer"}
impl_primitive_type! { isize, "isize", "integer"}
impl_primitive_type! { u8, "u8", "integer"}
impl_primitive_type! { u16, "u16", "integer"}
impl_primitive_type! { u32, "u32", "integer"}
impl_primitive_type! { u64, "u64", "integer"}
impl_primitive_type! { usize, "usize", "integer"}
impl_primitive_type! { String, "string", "string"}
impl_primitive_type! { bool, "string", "boolean"}
impl_primitive_type! { f32, "string", "number"}
impl_primitive_type! { f64, "string", "number"}

impl Schematic for () {
    fn name() -> &'static str {
        "void"
    }

    fn required() -> bool {
        false
    }  

    fn type_() -> &'static str {
        "void"
    }
}

impl Schematic for &str {
    fn name() -> &'static str {
        "string"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl Schematic for uuid::Uuid {
    fn name() -> &'static str {
        "uuid"
    }
    fn required() -> bool {
        true
    }
    fn format() -> Option<String> {
        Some("uuid".to_string())
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl Schematic for chrono::NaiveDateTime {
    fn name() -> &'static str {
        "datetime"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("date-time".to_string())
    }
}

impl Schematic for chrono::NaiveDate {
    fn name() -> &'static str {
        "date"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }

    fn format() -> Option<String> {
        Some("date".to_string())
    }
}

impl Schematic for serde_json::Value {
    fn name() -> &'static str {
        "object"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn format() -> Option<String> {
        Some("json".to_string())
    }
}

impl<T: Schematic> Schematic for Option<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        false
    }

    fn type_() -> &'static str {
        T::type_()
    }

    fn doc() -> Option<String> {
        T::doc()
    }
    fn generate_schema() -> EnhancedSchema {
        T::generate_schema()
    }
}

impl<T: Schematic> Schematic for &T {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        T::required()
    }

    fn type_() -> &'static str {
        T::type_()
    }
    fn doc() -> Option<String> {
        T::doc()
    }
    fn generate_schema() -> EnhancedSchema {
        T::generate_schema()
    }
}

impl<T: Schematic> Schematic for Vec<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        T::required()
    }

    fn type_() -> &'static str {
        "array"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().to_string()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
    }
}

impl Schematic for BigDecimal {
    fn name() -> &'static str {
        "string"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl<T: Schematic> Schematic for HashSet<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "array"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().to_string()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        };
        schema.schema.extras.insert("items".to_string(), T::generate_schema().schema.to_value());
        schema
    }
}

impl<K: ToString, V: Schematic> Schematic for HashMap<K, V> {
    fn name() -> &'static str {
        V::name()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }

    fn generate_schema() -> EnhancedSchema {
        let mut schema = EnhancedSchema {
            schema: Schema {
                _type: Some(Self::type_().to_string()),
                format: None,
                nullable: None,
                description: Self::doc(),
                extras: Default::default(),
            },
            required: Self::required(),
        };
        let mut properties = BTreeMap::new();
        properties.insert("type".to_string(), "string".to_string());
        properties.insert("format".to_string(), V::type_().to_string());
        schema
            .schema
            .extras
            .insert("additionalProperties".to_string(), ::serde_json::to_value(properties).unwrap());
        schema
    }
}

impl Schematic for DateTime<Utc> {
    fn name() -> &'static str {
        "string"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
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
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        let mut ret = vec![];
        let mut schema = T::generate_schema();
        if let Some(mut properties) = schema.schema.extras.remove("properties") {
            if let Some(properties) = properties.as_object_mut() {
                properties.iter_mut().for_each(|(key, value)| {
                    let schema = serde_json::from_value(value.clone()).unwrap();
                    let param = build_param(key.to_string(), ParameterIn::Path, T::required(), schema, T::doc());
                    ret.push(param);
                })
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
        let mut ret = vec![];
        let mut schema = T::generate_schema();
        if let Some(mut properties) = schema.schema.extras.remove("properties") {
            if let Some(properties) = properties.as_object_mut() {
                properties.iter_mut().for_each(|(key, value)| {
                    let schema = serde_json::from_value(value.clone()).unwrap();
                    let param = build_param(key.to_string(), ParameterIn::Path, T::required(), schema, T::doc());
                    ret.push(param);
                })
            }
        }
        Either::Left(ret)
    }
}

impl<T> ParameterProvider for State<T> {}

impl ParameterProvider for Request {}
