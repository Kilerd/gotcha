use actix_web::web;
use async_trait::async_trait;
use std::sync::Arc;

pub struct Messager {}

pub type MessagerWrapper = web::Data<Messager>;

impl Messager {
    pub async fn send<T: Message>(self: Arc<Self>, msg: T) -> T::Output {
        msg.handle(self).await
    }

    pub async fn spawn<T>(self: Arc<Self>, msg: T) -> ()
    where
        T: Message + 'static,
        T::Output: Send,
    {
        tokio::spawn(msg.handle(self));
    }
}

#[async_trait]
pub trait Message {
    type Output;
    async fn handle(self, messager: Arc<Messager>) -> Self::Output;
}
