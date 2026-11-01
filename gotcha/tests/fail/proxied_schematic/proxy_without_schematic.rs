use gotcha::Schematic;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct RawColor {
    hex: String,
}

#[derive(Schematic, Serialize, Deserialize, Clone)]
#[serde(from = "RawColor", into = "RawColor")]
struct Color {
    red: u8,
}

impl From<RawColor> for Color {
    fn from(_: RawColor) -> Self {
        Color { red: 0 }
    }
}

impl From<Color> for RawColor {
    fn from(color: Color) -> Self {
        RawColor {
            hex: color.red.to_string(),
        }
    }
}

fn main() {}
