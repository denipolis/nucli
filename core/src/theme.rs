use gpui::{Hsla, rgb};

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ItemColor {
    Red,
    Orange,
    Yellow,
    Green,
    Mint,
    Teal,
    Blue,
    Indigo,
    Purple,
    Pink,
    Brown,
    Gray,
}

impl ItemColor {
    pub const ALL: [ItemColor; 12] = [
        ItemColor::Red,
        ItemColor::Orange,
        ItemColor::Yellow,
        ItemColor::Green,
        ItemColor::Mint,
        ItemColor::Teal,
        ItemColor::Blue,
        ItemColor::Indigo,
        ItemColor::Purple,
        ItemColor::Pink,
        ItemColor::Brown,
        ItemColor::Gray,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ItemColor::Red => "Red",
            ItemColor::Orange => "Orange",
            ItemColor::Yellow => "Yellow",
            ItemColor::Green => "Green",
            ItemColor::Mint => "Mint",
            ItemColor::Teal => "Teal",
            ItemColor::Blue => "Blue",
            ItemColor::Indigo => "Indigo",
            ItemColor::Purple => "Purple",
            ItemColor::Pink => "Pink",
            ItemColor::Brown => "Brown",
            ItemColor::Gray => "Gray",
        }
    }

    pub fn color(self, dark: bool) -> Hsla {
        let (on_dark, on_light) = match self {
            ItemColor::Red => (0xFF6B6B, 0xC62828),
            ItemColor::Orange => (0xFF9F43, 0xD35400),
            ItemColor::Yellow => (0xF7D154, 0x9A7B00),
            ItemColor::Green => (0x6BCB77, 0x2E7D32),
            ItemColor::Mint => (0x63E6BE, 0x0B8A6A),
            ItemColor::Teal => (0x4FC3C7, 0x00796B),
            ItemColor::Blue => (0x5FA8FF, 0x1565C0),
            ItemColor::Indigo => (0x8C8CFF, 0x3F3FBF),
            ItemColor::Purple => (0xC387FF, 0x7B1FA2),
            ItemColor::Pink => (0xFF79C6, 0xC2185B),
            ItemColor::Brown => (0xC49A6C, 0x795548),
            ItemColor::Gray => (0x9E9E9E, 0x616161),
        };
        rgb(if dark { on_dark } else { on_light }).into()
    }
}
