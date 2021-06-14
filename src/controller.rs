<<<<<<< HEAD
use std::future::Future;
use async_trait::async_trait;
use std::marker::PhantomData;
use tower_service::Service;
use futures_util::core_reexport::task::{Context, Poll};
use std::convert::Infallible;
use hyper::{Request, Body};
use futures_util::core_reexport::pin::Pin;
use pin_project::pin_project;

//use std::future::Future;
//use futures_util::try_future::IntoFuture;
//use futures_util::FutureExt;
//
pub struct GotchaResponse {}

pub struct HttpRequest {}

//
////use http_service::{Body, Response};
////
pub trait Responder {
    fn to_response(&self) -> GotchaResponse
        where
            Self: Sized;
}

//
impl Responder for String {
    fn to_response(&self) -> GotchaResponse
        where
            Self: Sized,
    {
        GotchaResponse {}
    }
}


pub trait FromRequest {
    fn from_request() -> Self
        where
            Self: Sized;
}


pub trait HandlerFactory<P, RES, OUT>: Clone + 'static
    where
        RES: Future<Output=OUT>,
        OUT: Responder,
{
    fn build_params(&self) -> P;
    fn call(&self, _: P) -> RES;
}


impl<F, RES, OUT> HandlerFactory<(), RES, OUT> for F
    where
        F: Fn() -> RES + Clone + 'static,
        RES: Future<Output=OUT>,
        OUT: Responder,
{
    fn build_params(&self) -> () {
        ()
    }

    fn call(&self, _: ()) -> RES {
        (self)()
    }
}


pub struct HandlerController<FACTOR, P, RES, OUT>
    where
        FACTOR: HandlerFactory<P, RES, OUT>,
        RES: Future<Output=OUT>,
        OUT: Responder,
{
    pub hnd: FACTOR,
    _t: PhantomData<(P, RES, OUT)>,
}

impl<FACTOR, P, RES, OUT> HandlerController<FACTOR, P, RES, OUT>
    where
        FACTOR: HandlerFactory<P, RES, OUT>,
        RES: Future<Output=OUT>,
        OUT: Responder, {
    pub fn new(hnd: FACTOR) -> Self {
        HandlerController { hnd, _t: PhantomData }
    }
}

impl<FACTOR, P, RES, OUT> Clone for HandlerController<FACTOR, P, RES, OUT>
    where
        FACTOR: HandlerFactory<P, RES, OUT>,
        RES: Future<Output=OUT>,
        OUT: Responder,
{
    fn clone(&self) -> Self {
        HandlerController {
            hnd: self.hnd.clone(),
            _t: PhantomData,
        }
    }
}

impl<FACTOR, P, RES, OUT, > Service<Request<Body>> for HandlerController<FACTOR, P, RES, OUT>
    where
        FACTOR: HandlerFactory<P, RES, OUT>,
        RES: Future<Output=OUT>,
        OUT: Responder,
{
    type Response = GotchaResponse;
    type Error = Infallible;
    type Future = HandlerServiceResponse<RES, OUT>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let p = self.hnd.build_params();
        HandlerServiceResponse {
            fut: self.hnd.call(p),
            req: HttpRequest {},
        }
    }
}

#[pin_project]
pub struct HandlerServiceResponse<RES, OUT>
    where RES: Future<Output=OUT>,
          OUT: Responder {
    #[pin]
    fut: RES,
    req: HttpRequest,
}

impl<RES, OUT> Future for HandlerServiceResponse<RES, OUT>
    where RES: Future<Output=OUT>,
          OUT: Responder
{
    type Output = Result<GotchaResponse, Infallible>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let x = self.project();
        match x.fut.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(res) => {
                let response = res.to_response();
                Poll::Ready(Ok(response))
            }
        }
    }
}








=======
// use std::future::Future;
// use futures_util::try_future::IntoFuture;
// use futures_util::FutureExt;
//
// pub struct GotchaResponse {}
//
// pub trait Responder {
//     fn to_response(&self) -> GotchaResponse
//         where
//             Self: Sized;
// }
//
// impl Responder for String {
//     fn to_response(&self) -> GotchaResponse
//         where
//             Self: Sized,
//     {
//         GotchaResponse {}
//     }
// }
//
// pub trait FromRequest {
//     fn from_request() -> Self
//         where
//             Self: Sized;
// }
>>>>>>> 461e5a5 (draft)
//
//
// pub trait HandlerFactory<P, R>
//     where
//         R: Future<Output=GotchaResponse>,
// {
//     fn build_params(&self) -> P;
//     fn call(&self, _: P) -> R;
// }
//
//
// impl<F, RES> HandlerFactory<(), R> for F
//     where
//         F: Fn() -> RES,
//         RES: Future, RES::Output: Responder,
//         R: Future<Output=GotchaResponse>,
// {
//     fn build_params(&self) -> () {
//         ()
//     }
//
<<<<<<< HEAD

=======
//     fn call(&self, _: ()) -> R {
//         (self)().map(|x| x.to_response())
//     }
// }
//
//
>>>>>>> 461e5a5 (draft)
// macro_rules! factory_tuple ({ $(($n:tt, $T:ident)),+} => {
//     impl<F, RES, $($T,)+> HandlerFactory<($($T,)+), RES> for F
//     where F: Fn($($T,)+) -> RES,
//     RES: Future, RES::Output : Responder,
//     $($T : FromRequest,)+
//     {
//         fn build_params(&self) -> ($($T,)+) {
//         ($($T::from_request(),)+)
//     }
//
//         fn call(&self, param: ($($T,)+)) -> RES {
//             (self)($(param.$n,)+)
//         }
//     }
// });
//
// factory_tuple!((0, P0));
// factory_tuple!((0, P0), (1, P1));
<<<<<<< HEAD
////factory_tuple!((0, P0), (1, P1), (2, P2));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9), (10, P10));
=======
// factory_tuple!((0, P0), (1, P1), (2, P2));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9));
// factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9), (10, P10));

use async_trait::async_trait;
use std::future::Future;
use crate::app::Responder;

use hyper::{Body, Request, Response, Server};
use std::pin::Pin;

pub struct GotchaResponse(pub String);

impl GotchaResponse {
    pub fn into_response(self) -> Response<Body> {
        Response::builder()
            .status(200)
            .body(Body::from(Vec::from(self.0.as_bytes())))
            .unwrap()
    }
}

#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn call(&self) -> GotchaResponse;
}


#[async_trait]
impl<F, RES, > Handler for F
    where F: (Fn() -> RES) + Sync + Send + 'static,
          RES: Future + Send, RES::Output: Responder,
{
    async fn call(&self) -> GotchaResponse {
        self().await.into_response()
    }
}

#[async_trait]
impl<F, RES, P> Handler for F
    where F: (Fn(P) -> RES) + Sync + Send + 'static,
          RES: Future + Send,
          RES::Output: Responder,
          P: FromRequest,
{
    async fn call(&self) -> GotchaResponse {
        let p0 = P::from_request().await;
        self(p0).await.into_response()
    }
}

#[async_trait]
pub trait FromRequest {
    async fn from_request() -> Self;
}

#[async_trait]
impl FromRequest for () {
    async fn from_request() -> Self {
        ()
    }
}

#[async_trait]
impl FromRequest for String {
    async fn from_request() -> Self {
        "hello string".to_owned()
    }
}


pub async fn async_handler1() -> String {
    "hello world".to_owned()
}

pub async fn async_handler2(input: String) -> String {
    input
}
>>>>>>> 461e5a5 (draft)
