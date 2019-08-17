use hyper::{Server, Request, Body, Response};
use hyper::server::Builder;
use hyper::server::conn::AddrIncoming;
use hyper::service::service_fn_ok;
use std::net::ToSocketAddrs;
use hyper::rt::Future;
use crate::data::DateContainer;
pub struct App {
    data_container:  DateContainer
}

fn hello_world(_req: Request<Body>) -> Response<Body> {
    Response::new(Body::from("hello world"))
}

impl App {

    pub fn new() -> Self {
        Self {
            data_container : DateContainer::new()
        }
    }

    pub fn data<T: 'static>(mut self, data: T) -> Self {
        self.data_container.insert(data);
        self
    }

    pub fn run(self, addr: impl std::net::ToSocketAddrs) -> Result<(), std::io::Error> {
        let new_svc = || {
            // service_fn_ok converts our function into a `Service`
            service_fn_ok(hello_world)
        };

        let socket_addr = addr.to_socket_addrs()?.next().ok_or(std::io::ErrorKind::InvalidInput)?;
        let server = Server::bind(&socket_addr)
            .serve(new_svc)
            .map_err(|e| panic!("{}", e));

        Ok(hyper::rt::run(server))
    }
}