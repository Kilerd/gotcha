use std::collections::BTreeMap;
use http::Method;
use oas::{Operation, Parameter, Responses};

impl ApiObject for String {
    fn name() -> &'static str {
        unimplemented!()
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "string"
    }
}

impl ApiObject for i32 {
    fn name() -> &'static str {
        "integer"
    }

    fn required() -> bool {
        true
    }

    fn type_() -> &'static str {
        "integer"
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
