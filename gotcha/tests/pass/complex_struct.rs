use gotcha::{Schematic};


#[derive(Schematic)]
pub struct Pagination {
    page: usize,
    size: usize,
    option_string: Option<String>,
    data: Option<Vec<u8>>,

}

fn main() {
    let _operation = Pagination::generate_schema();
    assert!(Pagination::name().eq("Pagination"));
}