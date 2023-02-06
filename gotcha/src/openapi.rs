use actix_web::dev::Payload;
use actix_web::web::{Data, Json, Path};
use actix_web::{FromRequest, HttpRequest, HttpResponse};
use http::Method;
use oas::{OpenAPIV3, Operation, Parameter, ParameterIn, Responses, Schema};


pub trait ApiObject {
    fn name() -> &'static str;
    fn required() -> bool;
    fn type_() -> &'static str;
    fn generate() -> Parameter {
        Parameter {
            name: Self::name().to_string(),
            _in: ParameterIn::Path,
            description: None,
            required: Some(Self::required()),
            deprecated: Some(false),
            allow_empty_value: None,
            style: None,
            explode: None,
            allow_reserved:None,
            schema: Some(oas::Referenceable::Data(Schema{
                _type: Some(Self::type_().to_string()),
                format:None,
                nullable:None,
                extras:Default::default()
            })),
            example: None,
            examples: None,
            content:None
        }
        // Parameter {
        //     name: 
        //     _in: ParameterIn::Query,

        // }

    }
}
pub trait ParameterProvider {
    fn location() -> ParameterIn;
    fn generate(url: String) -> Option<Vec<Parameter>>;
}

macro_rules! impl_primitive_type {
    ($t: ty, $name: expr, $api_type: expr) => {
        impl ApiObject for $t {
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
    }
}

impl_primitive_type!{ i8, "i32", "number"}
impl_primitive_type!{ i16, "i16", "number"}
impl_primitive_type!{ i32, "i32", "number"}
impl_primitive_type!{ i64, "i64", "number"}
impl_primitive_type!{ isize, "isize", "number"}
impl_primitive_type!{ u8, "u8", "number"}
impl_primitive_type!{ u16, "u16", "number"}
impl_primitive_type!{ u32, "u32", "number"}
impl_primitive_type!{ u64, "u64", "number"}
impl_primitive_type!{ usize, "usize", "number"}
impl_primitive_type!{ String, "string", "string"}




impl<T: ApiObject> ApiObject for Option<T> {
    
    fn name() -> &'static str {
        T::name()
    }

    fn required() -> bool {
        false
    }

    fn type_() -> &'static str {
        T::type_()
    }
}

struct MyRequest {
    name: String,
    fav_number: i32,
}

impl ApiObject for MyRequest {
    fn name() -> &'static str {
        "MyRequest"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "object"
    }
}

impl<T1:ApiObject> ParameterProvider for Path<(T1, )> {

  
    fn location() -> ParameterIn {
        ParameterIn::Path
    }
    fn generate(url: String) -> Option<Vec<Parameter>> {
        let pattern = regex::Regex::new(r"\{([^\}]+)\}").unwrap();
        let param_names_in_path: Vec<String> = pattern.captures_iter(&url)
        .map(|digits| dbg!(digits.get(1).unwrap()).as_str().to_string())
        .collect();

        let mut t1_param = T1::generate();
        t1_param._in = Self::location();
        t1_param.name = param_names_in_path.get(0).cloned().expect("cannot get param in path");

        Some(vec![t1_param])
    }
}

impl<T1:ApiObject, T2: ApiObject> ParameterProvider for Path<(T1, T2)> {

  
    fn location() -> ParameterIn {
        ParameterIn::Path
    }
    fn generate(url: String) -> Option<Vec<Parameter>> {
        let pattern = regex::Regex::new(r"\{([^\}]+)\}").unwrap();
        let param_names_in_path: Vec<String> = pattern.captures_iter(&url)
        .map(|digits| dbg!(digits.get(1).unwrap()).as_str().to_string())
        .collect();

        let mut t1_param = T1::generate();
        t1_param._in = Self::location();
        t1_param.name = param_names_in_path.get(0).cloned().expect("cannot get param in path");


        let mut t2_param = T2::generate();
        t2_param._in = Self::location();
        t2_param.name = param_names_in_path.get(1).cloned().expect("cannot get param in path");

        Some(vec![t1_param, t2_param])
    }
}

impl<T: ApiObject> ParameterProvider for Path<T> {
    fn location() -> ParameterIn {
        ParameterIn::Path
    }
    fn generate(url: String) -> Option<Vec<Parameter>> {
        todo!()
    }
}

impl<T> ParameterProvider for Data<T> {
    fn location() -> ParameterIn {
        ParameterIn::Path
    }
    fn generate(url: String) -> Option<Vec<Parameter>> {
        None
    }
}


pub(crate) async fn openapi_handler(spec: Data<OpenAPIV3>) -> Json<OpenAPIV3> {
    Json(spec.get_ref().clone())
}

pub(crate) async fn openapi_html() -> HttpResponse {
    HttpResponse::Ok().content_type("text/html; charset=UTF-8").body(include_str!("../statics/redoc.html"))
}
