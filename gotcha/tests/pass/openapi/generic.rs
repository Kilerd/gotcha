use gotcha::Schematic;

#[derive(Schematic)]
pub struct ResponseWrapper<T: Schematic> {
    pub code: i32,
    pub message: String,
    pub data: T,
}

fn main() {}
