use std::collections::BTreeMap;
use actix_web::Either;

use actix_web::web::{Data, Json, Path, Query};
use convert_case::{Case, Casing};
use http::Method;
use oas::{MediaType, Operation, Parameter, ParameterIn, Referenceable, RequestBody, Response, Responses, Schema};

pub trait Operable {
    fn should_generate_openapi_spec(&self) -> bool {
        true
    }
    fn id(&self) -> &'static str;
    fn method(&self) -> Method;
    fn uri(&self) -> &'static str;
    fn group(&self) -> Option<String>;
    fn description(&self) -> Option<&'static str>;
    fn deprecated(&self) -> bool;
    fn generate(&self) -> Operation {
        let tags = if let Some(group) = self.group() { Some(vec![group]) } else { None };

        let mut params = vec![];
        let mut request_body = None;
        let vec1 = self.parameters();
        for item in vec1 {
            match item {
                Either::Left(params_vec) => { params.extend(params_vec.into_iter().map(|param| Referenceable::Data(param))); }
                Either::Right(req_body) => { request_body = Some(Referenceable::Data(req_body)) }
            }
        }
        Operation {
            tags,
            summary: Some(self.id().to_case(Case::Title)),
            description: self.description().map(|v| v.to_string()),
            external_docs: None,
            operation_id: Some(self.id().to_string()),
            parameters: Some(params),
            request_body: request_body,
            responses: Responses {
                default: Some(Referenceable::Data(Response {
                    description: "default return".to_string(),
                    headers: None,
                    content: None,
                    links: None,
                })),
                data: BTreeMap::default(),
            },
            callbacks: None,
            deprecated: Some(self.deprecated()),
            security: None,
            servers: None,
        }
    }

    fn parameters(&self) -> Vec<Either<Vec<Parameter>, RequestBody>>;
}

pub trait Schematic {
    fn name() -> &'static str;
    fn required() -> bool;
    fn type_() -> &'static str;
    fn generate_schema() -> Schema {
        Schema {
            _type: Some(Self::type_().to_string()),
            format: None,
            nullable: None,
            extras: Default::default(),
        }
    }
}

pub trait ParameterProvider {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody>;
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

impl_primitive_type! { i8, "i32", "number"}
impl_primitive_type! { i16, "i16", "number"}
impl_primitive_type! { i32, "i32", "number"}
impl_primitive_type! { i64, "i64", "number"}
impl_primitive_type! { isize, "isize", "number"}
impl_primitive_type! { u8, "u8", "number"}
impl_primitive_type! { u16, "u16", "number"}
impl_primitive_type! { u32, "u32", "number"}
impl_primitive_type! { u64, "u64", "number"}
impl_primitive_type! { usize, "usize", "number"}
impl_primitive_type! { String, "string", "string"}
impl_primitive_type! { bool, "string", "boolean"}

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
    fn generate_schema() -> Schema {
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
    fn generate_schema() -> Schema {
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

    fn generate_schema() -> Schema {
        let mut schema = Schema {
            _type: Some(Self::type_().to_string()),
            format: None,
            nullable: None,
            extras: Default::default(),
        };
        schema.extras.insert("items".to_string(), T::generate_schema().to_value());
        schema
    }
}

fn build_param(name: String, _in: ParameterIn, required: bool, schema: Schema) -> Parameter {
    Parameter {
        name,
        _in,
        description: None,
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

impl<T1: Schematic> ParameterProvider for Path<(T1, )> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        let pattern = regex::Regex::new(r"\{([^\}]+)\}").unwrap();
        let param_names_in_path: Vec<String> = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string()).collect();

        let t1_param = build_param(
            param_names_in_path.get(0).cloned().expect("cannot get param in path"),
            ParameterIn::Path,
            T1::required(),
            T1::generate_schema(),
        );
        Either::Left(vec![t1_param])
    }
}

impl<T1: Schematic, T2: Schematic> ParameterProvider for Path<(T1, T2)> {
    fn generate(url: String) -> Either<Vec<Parameter>, RequestBody> {
        let pattern = regex::Regex::new(r"\{([^\}]+)\}").unwrap();
        let param_names_in_path: Vec<String> = pattern.captures_iter(&url).map(|digits| digits.get(1).unwrap().as_str().to_string()).collect();

        let t1_param = build_param(
            param_names_in_path.get(0).cloned().expect("cannot get param in path"),
            ParameterIn::Path,
            T1::required(),
            T1::generate_schema(),
        );
        let t2_param = build_param(
            param_names_in_path.get(1).cloned().expect("cannot get param in path"),
            ParameterIn::Path,
            T2::required(),
            T2::generate_schema(),
        );

        Either::Left(vec![t1_param, t2_param])
    }
}

impl<T: Schematic> ParameterProvider for Path<T> {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        let mut ret = vec![];
        let mut schema = T::generate_schema();
        if let Some(mut properties) = schema.extras.remove("properties") {
            if let Some(properties) = properties.as_object_mut() {
                properties.iter_mut().for_each(|(key, value)| {
                    let schema = serde_json::from_value(value.clone()).unwrap();
                    let param = build_param(key.to_string(), ParameterIn::Path, T::required(), schema);
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
        contents.insert("application/json".to_owned(), MediaType {
            schema: Some(Referenceable::Data(schema)),
            example: None,
            examples: None,
            encoding: None,
        });
        let req_body = RequestBody {
            description: None,
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
        if let Some(mut properties) = schema.extras.remove("properties") {
            if let Some(properties) = properties.as_object_mut() {
                properties.iter_mut().for_each(|(key, value)| {
                    let schema = serde_json::from_value(value.clone()).unwrap();
                    let param = build_param(key.to_string(), ParameterIn::Path, T::required(), schema);
                    ret.push(param);
                })
            }
        }
        Either::Left(ret)
    }
}

impl<T> ParameterProvider for Data<T> {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        Either::Left(vec![])
    }
}

impl ParameterProvider for actix_web::HttpRequest {
    fn generate(_url: String) -> Either<Vec<Parameter>, RequestBody> {
        Either::Left(vec![])
    }
}
