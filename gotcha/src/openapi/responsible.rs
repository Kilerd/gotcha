use std::collections::BTreeMap;

use axum::Json;
use oas::{MediaType, Referenceable, Response, Responses};

use crate::openapi::schematic::Schematic;

pub trait Responsible {
    fn response() -> Responses;
}

// todo: add response for ()

impl<T> Responsible for Json<T>
where
    T: Schematic,
{
    fn response() -> Responses {
        let response_schema = (T::generate_schema)();
        let mut response = Responses {
            default: None,
            data: BTreeMap::default(),
        };
        response.data.insert(
            "200".to_string(),
            Referenceable::Data(Response {
                description: "default return".to_string(),
                headers: None,
                content: Some(BTreeMap::from([(
                    "application/json".to_string(),
                    MediaType {
                        schema: Some(Referenceable::Data(response_schema.schema)),
                        example: None,
                        examples: None,
                        encoding: None,
                    },
                )])),
                links: None,
            }),
        );
        response
    }
}

impl<T> Responsible for T
where
    T: Schematic,
{
    fn response() -> Responses {
        let response_schema = (T::generate_schema)();
        let mut response = Responses {
            default: None,
            data: BTreeMap::default(),
        };
        response.data.insert(
            "200".to_string(),
            Referenceable::Data(Response {
                description: "default return".to_string(),
                headers: None,
                content: Some(BTreeMap::from([(
                    "application/json".to_string(),
                    MediaType {
                        schema: Some(Referenceable::Data(response_schema.schema)),
                        example: None,
                        examples: None,
                        encoding: None,
                    },
                )])),
                links: None,
            }),
        );
        response
    }
}



impl<T, E> Responsible for Result<T, E>
where
    T: Responsible,
{
    fn response() -> Responses {
        let response = T::response();

        // todo: add error response
        response
    }
}
