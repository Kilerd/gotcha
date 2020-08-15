////use http_service::{Body, Response, HttpService};
//use crate::data::DateContainer;
//use crate::Middleware;
//use std::future::Future;
////use futures::future::{BoxFuture, Future};
//use std::collections::HashMap;
//use std::marker::PhantomData;
//
////
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::data::DateContainer;
use futures_util::future;
use http::StatusCode;
use hyper::service::Service;
use hyper::{Body, Request, Response, Server as HyperServer};
use tokio::runtime::Runtime;

const ROOT: &'static str = "/";

//
// pub trait Responder {}

//pub struct ControllerHolder {
//    func: Pin<Box<dyn FnMut(HttpRequest) -> Box<dyn Future<Output = HttpResponse> + Send> + Send>>,
//}
//
//impl Service<Request<Body>> for ControllerHolder {
//    type Response = Response<Body>;
//    type Error = hyper::Error;
//    type Future =
//        Pin<Box<dyn Future<Output = Result<Response<Body>, Self::Error>> + Send + 'static>>;
//
//    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//        Ok(()).into()
//    }
//
//    fn call(&mut self, req: Request<Body>) -> Self::Future {
//        //        let mut rsp = Response::builder();
//        //        let fut = async move {
//        //            let response = self.internel(HttpRequest).await;
//        //            let body = Body::from(Vec::from(&b"heyo!"[..]));
//        //            let rsp = rsp.status(200).body(body).unwrap();
//        //            rsp
//        //        };
//        //        Box::pin(fut)
//        //        let x = self.internel(HttpRequest);
//        let fut = async move {
//            //            let x = (self.func)(HttpRequest).await;
//            let response1 = hello_world(HttpRequest).await;
//            let mut rsp = Response::builder();
//            let body = Body::from("hello");
//            let rsp = rsp.status(200).body(body).unwrap();
//            Ok(rsp)
//        };
//
//        Box::pin(fut)
//    }
//}

#[derive(Debug)]
pub struct App {
    pub data_container: DateContainer,
    //    middlewares: Vec<Box<dyn Middleware + 'static + Send + Sync>>,
    //    router: Arc<Router<P>>
    pub msg: String,
}

//
////
////async fn hello(_: Request<Body>) -> Result<Response<Body>, hyper::Error> {
////    Ok(Response::new(Body::from("Hello World!")))
////}
////

impl App {
    pub fn new() -> Self {
        Self {
            data_container: DateContainer::new(),
            //            middlewares: Vec::new(),
            //            router: Arc::new(Router::new())
            msg: "hello world".into(),
        }
    }
    //
    //    pub fn data<T: 'static + Send + Sync>(mut self, data: T) -> Self {
    //        self.data_container.insert(data);
    //        self
    //    }
    //
    //    //    pub fn middleware(mut self, middleware: impl Middleware + 'static + Send + Sync) -> Self {
    //    //        self.middlewares.push(Box::new(middleware));
    //    //        self
    //    //    }
    //    //    //
    //    //    pub fn default_service(mut self, service: D) -> Self {
    //    //        self.defeault = Some(service);
    //    //        self
    //    //    }
    //    //
    //    pub async fn run(
    //        self,
    //        addr: impl std::net::ToSocketAddrs,
    //    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    //        let addr = ([127, 0, 0, 1], 1337).into();
    //        let server = HyperServer::bind(&addr).serve(Server);
    //        println!("Listening on http://{}", addr);
    //        Ok(server.await?)
    //    }
}
//
////impl<D, R> HttpService for App<D, R> where D: Fn() -> R + Send + Sync  + 'static, R:Future + Send + Sync  + 'static, R::Output: Responder + Send + Sync  + 'static {
////    type Connection = ();
////    type ConnectionFuture = futures::future::Ready<Result<(), std::io::Error>>;
////
////    fn connect(&self) -> Self::ConnectionFuture {
////        futures::future::ok(())
////    }
////
////    type ResponseFuture = BoxFuture<'static, Result<http_service::Response, ()>>;
////
////    fn respond(&self, conn: &mut Self::Connection, req: http_service::Request) -> Self::ResponseFuture {
////        let x = async move {
////            let d = self.defeault.as_ref().unwrap();
////            let x1 = (d)().await.to_response();
////            Ok(x1)
//////            Ok(self.defeault.unwrap().await.to_response())
//////            let response = http_service::Response::new(Body::from("hello world"));
////////            response
//////            Ok(response)
////        };
////        Box::pin(x)
////    }
////}
//
//#[derive(Debug)]
//pub enum Method {
//    Head,
//    Option,
//    Get,
//    Post,
//    Patch,
//    Put,
//    Delete,
//}
//
//pub struct Router {
//    handlers: HashMap<Method, Box<dyn HandlerFactory>>,
//}
//
//impl Router {
//    pub fn new() -> Self {
//        Self {
//            handlers: HashMap::default(),
//        }
//    }
//
//    pub fn to<H, P>(&mut self, method: Method, handler: H) -> &Self
//    where
//        H: HandlerFactory<P>,
//    {
//        println!("handle route {:?}", method);
//        self.handlers.insert(method, Box::new(handler));
//        self
//    }
//}
//
//pub trait FromRequest {
//    fn from_request() -> Self
//    where
//        Self: Sized;
//}
//
//impl FromRequest for String {
//    fn from_request() -> Self
//    where
//        Self: Sized,
//    {
//        "hello world".into()
//    }
//}
//
//pub trait HandlerFactory<P> {
//    fn call(&self, _: P) -> impl Responser;
//}
//
//impl<F> HandlerFactory<()> for F
//where
//    F: Fn() -> String,
//{
//    fn call(&self, _: ()) -> String {
//        (self)()
//    }
//}
///// FromRequest trait impl for tuples
//macro_rules! factory_tuple ({ $(($n:tt, $T:ident)),+} => {
//    impl<F, $($T,)+> HandlerFactory<($($T,)+)> for F
//    where F: Fn($($T,)+) -> String,
//    {
//        fn call(&self, param: ($($T,)+)) -> String {
//            (self)($(param.$n,)+)
//        }
//    }
//});
//
//factory_tuple!((0, A));
//factory_tuple!((0, A), (1, B));
//factory_tuple!((0, A), (1, B), (2, C));
