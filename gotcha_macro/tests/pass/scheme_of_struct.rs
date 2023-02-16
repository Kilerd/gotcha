use gotcha_core::{Operable,ApiObject};
use gotcha_macro::{Parameter};
use oas::{Parameter, Schema, Convertible};


#[derive(Parameter)]
pub struct Pagination {
    page: usize,
    size: usize
}

fn main() {
    let operation = Pagination::generate_schema();
    assert!(Pagination::name().eq("Pagination"));
}