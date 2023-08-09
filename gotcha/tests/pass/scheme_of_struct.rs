use gotcha::{Schematic};
use oas::{Parameter, Schema};


#[derive(Schematic)]
pub struct Pagination {
    page: usize,
    size: usize
}

fn main() {
    let operation = Pagination::generate_schema();
    assert!(Pagination::name().eq("Pagination"));
}