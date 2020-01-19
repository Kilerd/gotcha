//use std::future::Future;
//use futures_util::try_future::IntoFuture;
//use futures_util::FutureExt;
//
//pub struct GotchaResponse {
//
//}
//
////use http_service::{Body, Response};
////
//pub trait Responder {
//    fn to_response(&self) -> GotchaResponse
//    where
//        Self: Sized;
//}
//
//impl Responder for String {
//    fn to_response(&self) -> GotchaResponse
//    where
//        Self: Sized,
//    {
//        GotchaResponse{}
//    }
//}
////
////pub trait FromRequest {
////    fn from_request() -> Self
////    where
////        Self: Sized;
////}
////
////
////pub trait HandlerFactory<P ,R>
////where
////R : Future<Output=GotchaResponse>,
////{
////    fn build_params(&self) -> P;
////    fn call(&self, _: P) -> R;
////}
////
////
////impl<F, RES> HandlerFactory<(), R> for F
////    where
////        F: Fn() -> RES,
////        RES: Future, RES::Output: Responder,
////        R : Future<Output=GotchaResponse>,
////{
////    fn build_params(&self) -> () {
////        ()
////    }
////
////    fn call(&self, _: ()) -> R {
////        (self)().map(|x| x.to_response())
////    }
////}
//
//
//
//
//
////
////macro_rules! factory_tuple ({ $(($n:tt, $T:ident)),+} => {
////    impl<F, RES, $($T,)+> HandlerFactory<($($T,)+), RES> for F
////    where F: Fn($($T,)+) -> RES,
////    RES: Future, RES::Output : Responder,
////    $($T : FromRequest,)+
////    {
////        fn build_params(&self) -> ($($T,)+) {
////        ($($T::from_request(),)+)
////    }
////
////        fn call(&self, param: ($($T,)+)) -> RES {
////            (self)($(param.$n,)+)
////        }
////    }
////});
////
////factory_tuple!((0, P0));
////factory_tuple!((0, P0), (1, P1));
////factory_tuple!((0, P0), (1, P1), (2, P2));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9));
////factory_tuple!((0, P0), (1, P1), (2, P2), (3, P3), (4, P4), (5, P5), (6, P6), (7, P7), (8, P8), (9, P9), (10, P10));
