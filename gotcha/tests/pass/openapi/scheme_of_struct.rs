use gotcha::{Schematic};


#[derive(Schematic)]
pub struct Pagination {
    page: usize,
    size: usize
}

fn main() {
    let _operation = Pagination::generate_schema();
    assert!(Pagination::name().eq("Pagination"));
}