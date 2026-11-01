use gotcha::Schematic;

#[derive(Schematic)]
enum Example {
    #[schematic(unknown = "x")]
    A,
}

fn main() {}
