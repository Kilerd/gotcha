use std::collections::BTreeMap;
use axum::response::Html;
use either::Either;
use oas::{OpenAPIV3, Operation, Parameter, Referenceable, RequestBody, Response, Responses};
use convert_case::{Case, Casing};
use crate::Responder;

pub(crate) async fn openapi_html() -> impl Responder {
    Html(include_str!("../statics/redoc.html"))
}

#[derive(Debug, Clone)]
pub struct Operable {
    pub type_name: &'static str,
    pub id: &'static str,
    pub group: Option<&'static str>,
    pub description: Option<&'static str>,
    pub deprecated: bool,
    pub parameters: Vec<Either<Vec<Parameter>, RequestBody>>,
}


impl Operable {
    pub fn generate(self) -> Operation {
        let tags = if let Some(group) = self.group { Some(vec![group.to_string()]) } else { None };

        let mut params = vec![];
        let mut request_body = None;
        let vec1 = self.parameters;
        for item in vec1 {
            match item {
                Either::Left(params_vec) => { params.extend(params_vec.into_iter().map(|param| Referenceable::Data(param))); }
                Either::Right(req_body) => { request_body = Some(Referenceable::Data(req_body)) }
            }
        }
        Operation {
            tags,
            summary: Some(self.id.to_case(Case::Title)),
            description: self.description.map(|v| v.to_string()),
            external_docs: None,
            operation_id: Some(self.id.to_string()),
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
            deprecated: Some(self.deprecated),
            security: None,
            servers: None,
        }
    }
}


inventory::collect!(Operable);