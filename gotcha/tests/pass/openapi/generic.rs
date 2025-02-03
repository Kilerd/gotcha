use gotcha::Schematic;

#[derive(Schematic)]
pub struct ResponseWrapper<T: Schematic> {
    pub code: i32,
    pub message: String,
    pub data: T,
}

#[derive(Schematic)]
pub enum Either<A: Schematic, B: Schematic> {
    Left{left_data: A},
    Right{right_data: B},
}

fn main() {}
