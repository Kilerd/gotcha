//! # Message Module
//! 
//! This module provides a message handling system for Gotcha web applications.
//! It allows for the asynchronous handling of messages and the spawning of tasks to handle them.
//!
//! ## Features
//!
//! - Asynchronous message handling
//! - Task spawning for message processing
//! - Type-safe message handling
//!
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::State;

pub struct Messager {}

pub type MessagerWrapper = State<Messager>;

impl Messager {
    pub async fn send<T: Message>(self: Arc<Self>, msg: T) -> T::Output {
        msg.handle(self).await
    }

    pub async fn spawn<T>(self: Arc<Self>, msg: T)
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
