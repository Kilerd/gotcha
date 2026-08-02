//! The axum capabilities gotcha re-exports: `Form`, `Multipart`, SSE, WebSocket, custom
//! middleware, and `fallback_service`. These all existed in axum but had to be reached through
//! `gotcha::axum::…` (or, for websockets and SSE, needed a feature this crate did not enable).

use std::convert::Infallible;

use gotcha::prelude::*;

#[state]
#[derive(Clone, Default)]
struct AppState {}

#[derive(Clone, Default, Serialize, Deserialize)]
struct AppConfig {}

#[derive(Deserialize)]
struct Login {
    user: String,
}

// A urlencoded body.
async fn sign_in(Form(login): Form<Login>) -> impl Responder {
    login.user
}

// A multipart upload.
async fn upload(mut parts: Multipart) -> impl Responder {
    let mut names = Vec::new();
    while let Ok(Some(field)) = parts.next_field().await {
        names.push(field.name().unwrap_or_default().to_string());
    }
    names.join(",")
}

// Server-sent events.
async fn events() -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let stream = futures_util::stream::once(async { Ok(Event::default().data("tick")) });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// A websocket upgrade.
async fn socket(upgrade: WebSocketUpgrade) -> impl Responder {
    upgrade.on_upgrade(|mut socket: WebSocket| async move {
        // Echo one frame, then drop the socket.
        if let Some(Ok(frame)) = socket.recv().await {
            let _ = socket.send(frame).await;
        }
    })
}

// The route as the router matched it, and the URI before nesting rewrote it.
async fn introspect(matched: MatchedPath, OriginalUri(uri): OriginalUri) -> impl Responder {
    format!("{} {}", matched.as_str(), uri)
}

fn main() {
    let _app = Gotcha::with_types::<AppState, AppConfig>()
        .post("/sign-in", sign_in)
        .post("/upload", upload)
        .get("/events", events)
        .get("/socket", socket)
        .get("/introspect/{id}", introspect)
        // Custom middleware, without reaching into `gotcha::axum`.
        .layer(middleware::from_fn(|req: gotcha::axum::extract::Request, next: middleware::Next| async move {
            next.run(req).await
        }))
        .routes(|router| {
            // Unmatched requests can go to a `Service`, not just a handler.
            router.fallback_service(tower::service_fn(|_: gotcha::axum::extract::Request| async {
                Ok::<_, Infallible>(gotcha::axum::response::Response::new(gotcha::axum::body::Body::from("not found")))
            }))
        });
}
