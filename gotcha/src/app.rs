use hyper::{Server, Request, Body, Response};
use hyper::server::Builder;
use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::service::{service_fn, make_service_fn, MakeService, Service};
use std::net::ToSocketAddrs;
use std::future::Future;
use tokio::runtime::Runtime;
use std::marker::Send;

use crate::data::DateContainer;
use crate::middleware::Middleware;
use crate::controller::HandlerFactory;
use crate::Responder;


pub struct App<DH, P, RES> where DH: HandlerFactory<P, RES> + Send {
    data_container: DateContainer,
    middlewares: Vec<Box<dyn Middleware + 'static>>,
    service: Option<DH>
}

async fn hello(_: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    Ok(Response::new(Body::from("Hello World!")))
}

impl<DH, P ,RES> App<DH, P, RES> {
    pub fn new() -> Self {
        Self {
            data_container: DateContainer::new(),
            middlewares: Vec::new(),
            service: None
        }
    }

    pub fn data<T: 'static>(mut self, data: T) -> Self {
        self.data_container.insert(data);
        self
    }
    pub fn middleware(mut self, middleware: impl Middleware + 'static) -> Self {
        self.middlewares.push(Box::new(middleware));
        self
    }
    pub fn default_service(mut self, service: DH) -> Self where DH: HandlerFactory<P, RES> + Send, RES: Future, RES::Output : Responder {
        self.service = Some(service);
        self
    }

    pub fn run(self, addr: impl std::net::ToSocketAddrs) -> Result<(), std::io::Error> {
        let new_svc = make_service_fn(|socket: &AddrStream| {
            async {
                Ok::<_, hyper::Error>(service_fn(async move |rep| {
                    let dh = self.service.unwrap();
                    let x = dh.build_params();
                    dh.call(x).await.to_response()
                }))
            }
        });

        let socket_addr = addr.to_socket_addrs()?.next().ok_or(std::io::ErrorKind::InvalidInput)?;
        let server = Server::bind(&socket_addr)
            .serve(new_svc);

        let runtime = Runtime::new().expect("cannot start a tokio runtime");
        runtime.block_on(async {
            println!("Gotcha is listening on {}", socket_addr);
            server.await
        });
        Ok(())
    }
}

