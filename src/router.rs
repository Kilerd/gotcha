//use std::collections::HashMap;
//use http::Method;
////
////pub struct Router<P> {
////    handlers: HashMap<Method, Box<dyn HandlerFactory<P>>>,
////}
//
////impl<P> Router<P> {
////    pub fn new() -> Self {
////        Self {
////            handlers: HashMap::default(),
////        }
////    }
////
////    pub fn to<H>(&mut self, method: Method, handler: H) -> &Self
////    where
////        H: HandlerFactory<P> + 'static,
////    {
////        println!("handle route {:?}", method);
////        self.handlers.insert(method, Box::new(handler));
////        self
////    }
////}
