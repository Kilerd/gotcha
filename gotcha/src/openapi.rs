use std::collections::BTreeMap;

use axum::response::Html;
use convert_case::{Case, Casing};
use either::Either;
use oas::{Operation, Parameter, Referenceable, RequestBody, Response, Responses};
use once_cell::sync::Lazy;

use crate::Responder;

pub(crate) async fn openapi_html() -> impl Responder {
    Html(include_str!("../statics/redoc.html"))
}

pub type ParamType = Either<Vec<Parameter>, RequestBody>;

pub type ParamConstructor = Box<dyn Fn(String) -> ParamType + Sync + Send + 'static>;

#[derive()]
pub struct Operable {
    pub type_name: &'static str,
    pub id: &'static str,
    pub group: Option<&'static str>,
    pub description: Option<&'static str>,
    pub deprecated: bool,
    pub parameters: &'static Lazy<Vec<ParamConstructor>>,
}

impl Operable {
    pub fn generate(&self, path: String) -> Operation {
        let tags = self.group.map(|group| vec![group.to_string()]);

        let mut params = vec![];
        let mut request_body = None;
        for item in self.parameters.iter() {
            match item(path.clone()) {
                Either::Left(params_vec) => {
                    params.extend(params_vec.into_iter().map(|param| Referenceable::Data(param.clone())));
                }
                Either::Right(req_body) => request_body = Some(Referenceable::Data(req_body.clone())),
            }
        }
        Operation {
            tags,
            summary: Some(self.id.to_case(Case::Title)),
            description: self.description.map(|v| v.to_string()),
            external_docs: None,
            operation_id: Some(self.id.to_string()),
            parameters: Some(params),
            request_body,
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
