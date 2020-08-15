use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use futures_util::future;
use hyper::{Body, Request, Response, Server};
use hyper::service::Service;

use crate::app::{App};
use crate::controller::{Responder, HandlerController};

mod app;
mod controller;
mod data;
mod middleware;
mod router;

const ROOT: &str = "/";


struct Route(Box<dyn Fn(Request<Body>) -> dyn Future<Output=String>>);


async fn hello_world(req: Request<Body>) -> String {
    String::from("hello world")
}

async fn resp() -> impl Responder {
    String::from("hello world")
}


#[derive(Debug)]
pub struct GotchaConnection {
    app: Arc<App>,
}

impl Service<Request<Body>> for GotchaConnection {
    type Response = Response<Body>;
    type Error = hyper::Error;
    type Future =
    Pin<Box<dyn Future<Output=Result<Response<Body>, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        //        Box::pin(future::ok(rsp))
        let app = self.app.clone();
        let fut = async move {
            let mut rsp = Response::builder();
            let option: &AtomicUsize = app.data_container.get().unwrap();
            let original = option.fetch_add(1, Ordering::SeqCst);

            let string1 = hello_world(req).await;

            let string = format!("{} , click count {}", string1, original);
            let vec = Vec::from(string.as_bytes());
            let body = Body::from(vec);
            let rsp = rsp.status(200).body(body).unwrap();
            Ok(rsp)
        };

        Box::pin(fut)
    }
}

pub struct GotchaHttpService {
    app: Arc<App>,
}

impl<T> Service<T> for GotchaHttpService {
    type Response = GotchaConnection;
    type Error = std::io::Error;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, _: T) -> Self::Future {
        future::ok(GotchaConnection {
            app: self.app.clone(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:1337".parse().unwrap();

    let controller = HandlerController::new(resp);

    let x = (controller.hnd)().await;
    let response = x.to_response();


    let mut app = App::new();
    app.data_container.insert(AtomicUsize::new(0));
    let service = GotchaHttpService { app: Arc::new(app) };
    let server = Server::bind(&addr).serve(service);

    println!("Listening on http://{}", addr);

    server.await?;

    Ok(())
}
