use gotcha::{Operable,Schematic};
use oas::{Parameter, Schema};


#[derive(Schematic)]
pub struct Pagination {
    page: usize,
    size: usize,
    option_string: Option<String>,
    data: Option<Vec<u8>>,

}

fn main() {
    let operation = Pagination::generate_schema();
    assert!(Pagination::name().eq("Pagination"));
}