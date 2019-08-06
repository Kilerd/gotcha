#![feature(async_await)]
use runtime::net::TcpListener;
use std::io::Write;
use http::Response;
use futures::prelude::*;

#[runtime::main]
async fn main() -> std::io::Result<()> {
    let mut listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Listening on {}", listener.local_addr()?);

    // accept connections and process them in parallel
    listener
        .incoming()
        .try_for_each_concurrent(None, |stream| {
            async move {
                runtime::spawn(async move {
                    println!("Accepting from: {}", stream.peer_addr().unwrap());

                    let (reader, writer) = &mut stream.split();
                    let response = Response::builder().status(200).body("hello world").unwrap();
                    dbg!(response);

                    writer.write_all(r#"HTTP/1.1 200 OK
Date: Sun, 18 Oct 2009 08:56:53 GMT
Server: Apache/2.2.14 (Win32)
Last-Modified: Sat, 20 Nov 2004 07:16:26 GMT
ETag: "10000000565a5-2c-3e94b66c2e680"
Accept-Ranges: bytes
Content-Length: 44
Connection: close
Content-Type: text/html

<html><body><h1>It works!</h1></body></html>"#.as_bytes()).await;
                    Ok::<(), std::io::Error>(())
                })
                    .await
            }
        })
        .await.unwrap();
    Ok(())
}