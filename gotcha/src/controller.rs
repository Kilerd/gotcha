use http::Response;
use hyper::Body;
use tokio::prelude::Future;

pub trait Responder {
    fn to_response(self) -> Response<Body> where Self: Sized;
}

impl Responder for String {
    fn to_response(self) -> Response<Body> where Self: Sized {
        Response::builder().body(Body::from(self)).unwrap()
    }
}

pub trait FromRequest {
    fn from_request() -> Self where Self: Sized;
}


pub trait HandlerFactory<P, RES> where RES: Future, RES::Output : Responder {
    fn build_params(&self) -> P;
    fn call(&self, _: P) -> RES;
}


impl<F, RES> HandlerFactory<(), RES> for F where F: Fn() -> RES, RES: Future, RES::Output : Responder {
    fn build_params(&self) -> () {
        ()
    }

    fn call(&self, _: ()) -> RES {
        (self)()
    }
}

macro_rules! factory_tuple ({ $(($n:tt, $T:ident)),+} => {
    impl<F, RES, $($T,)+> HandlerFactory<($($T,)+), RES> for F
    where F: Fn($($T,)+) -> RES,
    RES: Future, RES::Output : Responder,
    $($T : FromRequest,)+
    {
        fn build_params(&self) -> ($($T,)+) {
        ($($T::from_request(),)+)
    }

        fn call(&self, param: ($($T,)+)) -> RES {
            (self)($(param.$n,)+)
        }
    }
});

factory_tuple!((0, P0));
factory_tuple!((0, P0), (1, P1));
factory_tuple!((0, P0), (1, P1), (2, P2));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9));
factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9), (10, P10));