use http_service::{Body, Response, HttpService};

use crate::data::DateContainer;
use crate::{Middleware, HandlerFactory, Responder};
use futures::future::{BoxFuture, Future};
use std::marker::PhantomData;

//
pub struct App<D, R> where D: Fn() -> R + Send + Sync  + 'static, R:Future + Send + Sync  + 'static, R::Output: Responder + Send + Sync  + 'static {
    data_container: DateContainer,
    middlewares: Vec<Box<dyn Middleware + 'static + Send + Sync>>,
    defeault: Option<D>,
}

//
//async fn hello(_: Request<Body>) -> Result<Response<Body>, hyper::Error> {
//    Ok(Response::new(Body::from("Hello World!")))
//}
//
impl<D, R> App<D, R>  where D: Fn() -> R + Send + Sync  + 'static, R:Future + Send + Sync  + 'static, R::Output: Responder + Send + Sync  + 'static {
    pub fn new() -> Self {
        Self {
            data_container: DateContainer::new(),
            middlewares: Vec::new(),

            defeault: None,
        }
    }

    pub fn data<T: 'static + Send + Sync>(mut self, data: T) -> Self {
        self.data_container.insert(data);
        self
    }

    pub fn middleware(mut self, middleware: impl Middleware + 'static + Send + Sync) -> Self {
        self.middlewares.push(Box::new(middleware));
        self
    }
    //
    pub fn default_service(mut self, service: D) -> Self  {
        self.defeault = Some(service);
        self
    }
//
    pub fn run(self, addr: impl std::net::ToSocketAddrs) -> Result<(), std::io::Error> {
//        let my_server = async move || {
//            let dh = self.service.unwrap();
//            let x1 = dh.call();
//            x1
//        };

//        let new_svc = make_service_fn(|socket: &AddrStream| {
//            async {
//                Ok::<_, hyper::Error>(service_fn(hello))
//            }
//        });
//
//        let socket_addr = addr.to_socket_addrs()?.next().ok_or(std::io::ErrorKind::InvalidInput)?;
//        let server = Server::bind(&socket_addr)
//            .serve(new_svc);
//
//        let runtime = Runtime::new().expect("cannot start a tokio runtime");
//        runtime.block_on(async {
//            println!("Gotcha is listening on {}", socket_addr);
//            server.await
//        });
//        Ok(())

//        let service = |req| {
//            futures::future::ok::<_, ()>(Response::new(Body::from("Hello World")))
//        };
        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or(std::io::ErrorKind::InvalidInput)?;
        Ok(http_service_hyper::run(self, addr))
    }
}

impl<D, R> HttpService for App<D, R> where D: Fn() -> R + Send + Sync  + 'static, R:Future + Send + Sync  + 'static, R::Output: Responder + Send + Sync  + 'static {
    type Connection = ();
    type ConnectionFuture = futures::future::Ready<Result<(), std::io::Error>>;

    fn connect(&self) -> Self::ConnectionFuture {
        futures::future::ok(())
    }

    type ResponseFuture = BoxFuture<'static, Result<http_service::Response, ()>>;

    fn respond(&self, conn: &mut Self::Connection, req: http_service::Request) -> Self::ResponseFuture {
        let x = async move {
            let d = self.defeault.as_ref().unwrap();
            let x1 = (d)().await.to_response();
            Ok(x1)
//            Ok(self.defeault.unwrap().await.to_response())
//            let response = http_service::Response::new(Body::from("hello world"));
////            response
//            Ok(response)
        };
        Box::pin(x)
    }
}

