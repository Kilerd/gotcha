use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

pub struct Data<T>(Arc<T>);

impl<T> Data<T> {
    pub fn new(data: T) -> Data<T> {
        Data(Arc::new(data))
    }
}

impl<T> Deref for Data<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}


pub(crate) struct DateContainer {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl DateContainer {
    #[inline]
    pub fn new() -> Self {
        Self {
            map: HashMap::new()
        }
    }

    pub fn insert<T: 'static>(&mut self, data: T) {
        self.map.insert(TypeId::of::<T>(), Box::new(data));
    }

    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    pub fn get<T: 'static>(&mut self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|boxed| (&**boxed as &(dyn Any + 'static)).downcast_ref())
    }

    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map.remove(&TypeId::of::<T>()).and_then(|boxed| {
            (boxed as Box<dyn Any + 'static>)
                .downcast()
                .ok()
                .map(|boxed| *boxed)
        })
    }
}

