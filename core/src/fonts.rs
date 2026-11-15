use std::borrow::Cow;

pub fn bundled() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/MapleMono-NF-BoldItalic.ttf")),
    ]
}
