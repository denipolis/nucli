use std::borrow::Cow;

pub const FAMILY: &str = "Maple Mono NF";

pub fn bundled() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-BoldItalic.ttf")),
    ]
}
