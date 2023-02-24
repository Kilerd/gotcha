use std::future::Future;
use std::pin::Pin;
use actix_web::{FromRequest, HttpRequest};
use actix_web::dev::Payload;
use gotcha_core::Schematic;
use actix_web::web::Json as ActixJson;
use futures::TryFutureExt;
use serde::de::DeserializeOwned;

pub struct Json<T>(T);

impl<T> Json<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> FromRequest for Json<T> where T: Schematic +  DeserializeOwned + 'static {
    type Error = actix_web::error::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let x = ActixJson::<T>::from_request(req, payload);
        Box::pin(async move {
            let t = x.await?;
            Ok(Json(t.into_inner()))
        })
    }
}