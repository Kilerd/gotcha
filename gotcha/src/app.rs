use hyper::{Server, Request, Body, Response};
use hyper::server::Builder;
use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::service::{service_fn, make_service_fn, MakeService, Service};
use std::net::ToSocketAddrs;
use hyper::rt::Future;
use tokio::runtime::Runtime;

use crate::data::DateContainer;


pub struct App {
    data_container:  DateContainer
}

async fn hello(_: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    Ok(Response::new(Body::from("Hello World!")))
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
        let new_svc = make_service_fn(|socket: &AddrStream|{
            async {
                Ok::<_, hyper::Error>(service_fn(hello))
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