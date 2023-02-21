use gotcha_core::{Operable,Schematic};
use gotcha_macro::{Schematic};
use oas::{Parameter, Schema, Convertible};


#[derive(Schematic)]
pub struct Pagination {
    page: usize,
    size: usize
}

fn main() {
    let operation = Pagination::generate_schema();
    assert!(Pagination::name().eq("Pagination"));
}