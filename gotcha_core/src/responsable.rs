use std::collections::BTreeMap;

use axum::Json;
use oas::{MediaType, Referenceable, Response, Responses};

use crate::Schematic;

pub trait Responsable {
    fn response() -> Responses;
}

impl<T> Responsable for Json<T> where T: Schematic {
    fn response() -> Responses {
        let response_schema = (T::generate_schema)();
        let mut response = Responses {
            default: None,
            data: BTreeMap::default(),
        };
        response.data.insert("200".to_string(), Referenceable::Data(Response {
            description: "default return".to_string(),
            headers: None,
            content: Some(BTreeMap::from([(
                "application/json".to_string(),
                MediaType {
                    schema: Some(Referenceable::Data(response_schema)),
                    example: None,
                    examples: None,
                    encoding: None,
                }
            )])),
            links: None,
        }));
        response
    }
}

impl<T> Responsable for T where T: Schematic {
    fn response() -> Responses {
        let response_schema = (T::generate_schema)();
        let mut response = Responses {
            default: None,
            data: BTreeMap::default(),
        };
        response.data.insert("200".to_string(), Referenceable::Data(Response {
            description: "default return".to_string(),
            headers: None,
            content: Some(BTreeMap::from([(
                "application/json".to_string(),
                MediaType {
                    schema: Some(Referenceable::Data(response_schema)),
                    example: None,
                    examples: None,
                    encoding: None,
                }
            )])),
            links: None,
        }));
        response
    }
}

impl Responsable for () {
    fn response() -> Responses {
        let mut response = Responses {
            default: None,
            data: BTreeMap::default(),
        };
        response.data.insert("204".to_string(), Referenceable::Data(Response {
            description: "no content".to_string(),
            headers: None,
            content: None,
            links: None,
        }));
        response
    }
}

impl<T, E> Responsable for Result<T, E> where T: Responsable {
    fn response() -> Responses {
        let response = T::response();
        
        // todo: add error response
        response
    }
}