use std::path::{Path, PathBuf};

use gpui::{App, Global, Hsla, Rgba, SharedString, rgb};
use serde::Deserialize;

use crate::paths;

#[derive(PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub background: Hsla,
    pub surface: Hsla,
    pub elevated: Hsla,
    pub border: Hsla,
    pub hover: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    pub selection: Hsla,
    pub red: Hsla,
    pub green: Hsla,
    pub yellow: Hsla,
    pub orange: Hsla,
    pub blue: Hsla,
    pub cyan: Hsla,
    pub added: Hsla,
    pub removed: Hsla,
    pub ansi: [Rgba; 16],
    pub foreground_rgb: Rgba,
    pub background_rgb: Rgba,
    pub syntax: Syntax,
}

#[derive(PartialEq)]
pub struct Syntax {
    pub keyword: Hsla,
    pub string: Hsla,
    pub number: Hsla,
    pub comment: Hsla,
    pub kind: Hsla,
    pub function: Hsla,
    pub constant: Hsla,
    pub attribute: Hsla,
    pub punctuation: Hsla,
}

#[derive(Clone, Copy)]
struct Palette {
    id: &'static str,
    name: &'static str,
    dark: bool,
    background: u32,
    foreground: u32,
    accent: u32,
    selection: u32,
    ansi: [u32; 16],
    syntax: [u32; 8],
}

const NUCLI_DARK: Palette = Palette {
    id: "nucli-dark",
    name: "Nucli Dark",
    dark: true,
    background: 0x111111,
    foreground: 0xD0D0D0,
    accent: 0x87AFD7,
    selection: 0x2F3A45,
    ansi: [
        0x1C1C1C, 0xD75F5F, 0x87AF87, 0xD7AF5F, 0x5F87AF, 0xAF87AF, 0x5FAFAF, 0xBCBCBC, 0x585858, 0xFF8787, 0xAFD787,
        0xFFD787, 0x87AFD7, 0xD7AFD7, 0x87D7D7, 0xEEEEEE,
    ],
    syntax: [0xD7875F, 0x87AF87, 0xAF87D7, 0x6C6C6C, 0x5FAFAF, 0x87AFD7, 0xD7AF5F, 0xAF87AF],
};

const NUCLI_LIGHT: Palette = Palette {
    id: "nucli-light",
    name: "Nucli Light",
    dark: false,
    background: 0xF4F1EA,
    foreground: 0x262626,
    accent: 0x005F87,
    selection: 0xD6E0E8,
    ansi: [
        0x262626, 0xAF3A3A, 0x3A7A3A, 0x8A6A00, 0x2A5A8A, 0x8A3A8A, 0x2A7A7A, 0xA8A8A8, 0x6C6C6C, 0xD75F5F, 0x5F875F,
        0xAF8700, 0x005F87, 0xAF5FAF, 0x008787, 0xD0D0D0,
    ],
    syntax: [0xAF5F00, 0x3A7A3A, 0x875FAF, 0x949494, 0x00747A, 0x005F87, 0x8A6A00, 0x8A3A8A],
};

const NUCLI_AMBER: Palette = Palette {
    id: "nucli-amber",
    name: "Nucli Amber",
    dark: true,
    background: 0x0D0900,
    foreground: 0xFFB000,
    accent: 0xFFB000,
    selection: 0x4A3300,
    ansi: [
        0x2A1D00, 0xFF6A00, 0xFFC94D, 0xFFB000, 0xCC8D00, 0xE69E00, 0xFFD27F, 0xFFB000, 0x664700, 0xFF8C33, 0xFFDB80,
        0xFFC233, 0xE6A100, 0xFFB733, 0xFFE0A6, 0xFFE9BF,
    ],
    syntax: [0xFFD27F, 0xFFC94D, 0xFFE0A6, 0x8A6000, 0xFFB000, 0xFFDB80, 0xFFC233, 0xCC8D00],
};

const NUCLI_PHOSPHOR: Palette = Palette {
    id: "nucli-phosphor",
    name: "Nucli Phosphor",
    dark: true,
    background: 0x030A05,
    foreground: 0x41FF7A,
    accent: 0x41FF7A,
    selection: 0x0F3A1D,
    ansi: [
        0x0B2412, 0xFF5F5F, 0x41FF7A, 0xC6FF5E, 0x2ED368, 0x7DFFA5, 0x9DFFBE, 0x41FF7A, 0x1F6B37, 0xFF8787, 0x8AFFAE,
        0xDAFF8F, 0x5CE68A, 0xA6FFC3, 0xC4FFD6, 0xE0FFEA,
    ],
    syntax: [0x9DFFBE, 0xC6FF5E, 0xDAFF8F, 0x1F8A45, 0x7DFFA5, 0xC4FFD6, 0xA6FFC3, 0x5CE68A],
};

const VITESSE_DARK_ANSI: [u32; 16] = [
    0x393A34, 0xCB7676, 0x4D9375, 0xE6CC77, 0x6394BF, 0xD9739F, 0x5EAAB5, 0xDBD7CA, 0x777777, 0xCB7676, 0x4D9375,
    0xE6CC77, 0x6394BF, 0xD9739F, 0x5EAAB5, 0xFFFFFF,
];

const VITESSE_LIGHT_ANSI: [u32; 16] = [
    0x121212, 0xAB5959, 0x1E754F, 0xBDA437, 0x296AA3, 0xA13865, 0x2993A3, 0xDBD7CA, 0xAAAAAA, 0xAB5959, 0x1E754F,
    0xBDA437, 0x296AA3, 0xA13865, 0x2993A3, 0xDDDDDD,
];

const VITESSE_DARK_SYNTAX: [u32; 8] = [0x4D9375, 0xC98A7D, 0x4C9A91, 0x758575, 0x5DA994, 0x80A665, 0xC99076, 0xBD976A];
const VITESSE_LIGHT_SYNTAX: [u32; 8] = [0x1E754F, 0xB56959, 0x2F798A, 0xA0ADA0, 0x2E8F82, 0x59873A, 0xA65E2B, 0xB07D48];

const VITESSE: [Palette; 5] = [
    Palette {
        id: "vitesse-dark",
        name: "Vitesse Dark",
        dark: true,
        background: 0x121212,
        foreground: 0xDBD7CA,
        accent: 0x4D9375,
        selection: 0x3A3A3A,
        ansi: VITESSE_DARK_ANSI,
        syntax: VITESSE_DARK_SYNTAX,
    },
    Palette {
        id: "vitesse-dark-soft",
        name: "Vitesse Dark Soft",
        dark: true,
        background: 0x222222,
        foreground: 0xDBD7CA,
        accent: 0x4D9375,
        selection: 0x444444,
        ansi: VITESSE_DARK_ANSI,
        syntax: VITESSE_DARK_SYNTAX,
    },
    Palette {
        id: "vitesse-black",
        name: "Vitesse Black",
        dark: true,
        background: 0x000000,
        foreground: 0xDBD7CA,
        accent: 0x4D9375,
        selection: 0x2A2A2A,
        ansi: VITESSE_DARK_ANSI,
        syntax: VITESSE_DARK_SYNTAX,
    },
    Palette {
        id: "vitesse-light",
        name: "Vitesse Light",
        dark: false,
        background: 0xFFFFFF,
        foreground: 0x393A34,
        accent: 0x1E754F,
        selection: 0xE4E4E4,
        ansi: VITESSE_LIGHT_ANSI,
        syntax: VITESSE_LIGHT_SYNTAX,
    },
    Palette {
        id: "vitesse-light-soft",
        name: "Vitesse Light Soft",
        dark: false,
        background: 0xF1F0E9,
        foreground: 0x393A34,
        accent: 0x1E754F,
        selection: 0xDDDCD3,
        ansi: VITESSE_LIGHT_ANSI,
        syntax: VITESSE_LIGHT_SYNTAX,
    },
];

pub const AUTOMATIC: [(&str, &str, &str, &str); 2] =
    [("nucli", "Nucli", "nucli-dark", "nucli-light"), ("vitesse", "Vitesse", "vitesse-dark", "vitesse-light")];

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hex(u32);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.strip_prefix('#')
            .filter(|hex| hex.len() == 6 && hex.chars().all(|char| char.is_ascii_hexdigit()))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .map(Hex)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid color \"{text}\", expected #rrggbb")))
    }
}

#[derive(Clone, Deserialize, Debug, PartialEq)]
pub struct CustomTheme {
    #[serde(default)]
    pub name: String,
    pub base: Option<String>,
    pub dark: Option<bool>,
    pub background: Option<Hex>,
    pub foreground: Option<Hex>,
    pub accent: Option<Hex>,
    pub selection: Option<Hex>,
    pub ansi: Option<[Hex; 16]>,
    pub syntax: Option<[Hex; 8]>,
}

#[derive(Default, PartialEq)]
pub struct Themes {
    pub custom: Vec<CustomTheme>,
    pub error: Option<String>,
}

impl Global for Themes {}

pub fn custom(cx: &App) -> &[CustomTheme] {
    cx.try_global::<Themes>().map_or(&[], |themes| &themes.custom)
}

pub fn reload(cx: &mut App) {
    let themes = load(&paths::themes_dir());
    if cx.try_global::<Themes>() != Some(&themes) {
        cx.set_global(themes);
    }
}

fn load(dir: &Path) -> Themes {
    let mut themes = Themes::default();
    let Ok(entries) = std::fs::read_dir(dir) else { return themes };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    files.sort();
    for file in files {
        match read(&file) {
            Ok(theme) => themes.custom.push(theme),
            Err(error) => {
                themes.error.get_or_insert(format!("themes/{error}"));
            }
        }
    }
    themes
}

fn read(path: &Path) -> Result<CustomTheme, String> {
    let file = path.file_name().unwrap_or_default().to_string_lossy();
    let text = std::fs::read_to_string(path).map_err(|error| format!("{file}: {error}"))?;
    let mut theme: CustomTheme = serde_json::from_str(&text).map_err(|error| format!("{file}: {error}"))?;
    if theme.name.is_empty() {
        theme.name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    }
    Ok(theme)
}

pub fn install(source: &Path, dir: &Path) -> Result<String, String> {
    let theme = read(source)?;
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let target = dir.join(format!("{stem}.json"));
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    if target != source {
        std::fs::copy(source, &target).map_err(|error| error.to_string())?;
    }
    Ok(theme.name)
}

impl CustomTheme {
    fn palette(&self) -> Palette {
        let base = self
            .base
            .as_deref()
            .and_then(|id| palettes().find(|palette| palette.id == id))
            .copied()
            .unwrap_or(if self.dark == Some(false) { NUCLI_LIGHT } else { NUCLI_DARK });
        let pick = |value: Option<Hex>, fallback: u32| value.map_or(fallback, |hex| hex.0);
        Palette {
            dark: self.dark.unwrap_or(base.dark),
            background: pick(self.background, base.background),
            foreground: pick(self.foreground, base.foreground),
            accent: pick(self.accent, base.accent),
            selection: pick(self.selection, base.selection),
            ansi: self.ansi.map_or(base.ansi, |ansi| ansi.map(|hex| hex.0)),
            syntax: self.syntax.map_or(base.syntax, |syntax| syntax.map(|hex| hex.0)),
            ..base
        }
    }
}

fn palettes() -> impl Iterator<Item = &'static Palette> {
    [&NUCLI_DARK, &NUCLI_LIGHT, &NUCLI_AMBER, &NUCLI_PHOSPHOR].into_iter().chain(VITESSE.iter())
}

pub fn all(custom: &[CustomTheme]) -> Vec<(SharedString, SharedString)> {
    AUTOMATIC
        .iter()
        .map(|(id, name, _, _)| ((*id).into(), (*name).into()))
        .chain(palettes().map(|palette| (palette.id.into(), palette.name.into())))
        .chain(custom.iter().map(|theme| (theme.name.clone().into(), theme.name.clone().into())))
        .collect()
}

pub fn is_automatic(id: &str) -> bool {
    AUTOMATIC.iter().any(|(pair, ..)| *pair == id)
}

pub fn resolve(id: &str, system_dark: bool, custom: &[CustomTheme]) -> Theme {
    let pair = AUTOMATIC.iter().find(|(pair, ..)| *pair == id);
    let id = match pair {
        Some((_, _, dark, light)) => {
            if system_dark {
                *dark
            } else {
                *light
            }
        }
        None => id,
    };
    let fallback = if system_dark { NUCLI_DARK } else { NUCLI_LIGHT };
    let palette = palettes()
        .find(|palette| palette.id == id)
        .copied()
        .or_else(|| custom.iter().find(|theme| theme.name == id).map(CustomTheme::palette))
        .unwrap_or(fallback);
    build(&palette)
}

fn mix(a: u32, b: u32, amount: f32) -> Hsla {
    let channel = |shift: u32| {
        let x = ((a >> shift) & 0xFF) as f32;
        let y = ((b >> shift) & 0xFF) as f32;
        (x + (y - x) * amount) / 255.0
    };
    Rgba { r: channel(16), g: channel(8), b: channel(0), a: 1.0 }.into()
}

fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn build(palette: &Palette) -> Theme {
    let bg = palette.background;
    let fg = palette.foreground;
    let ansi = palette.ansi;
    let tint = if palette.dark { 0.22 } else { 0.16 };
    Theme {
        dark: palette.dark,
        background: color(bg),
        surface: mix(bg, fg, 0.025),
        elevated: mix(bg, fg, 0.06),
        border: mix(bg, fg, 0.14),
        hover: mix(bg, fg, 0.08),
        text: color(fg),
        muted: mix(fg, bg, 0.38),
        faint: mix(fg, bg, 0.6),
        accent: color(palette.accent),
        on_accent: color(bg),
        selection: color(palette.selection),
        red: color(ansi[1]),
        green: color(ansi[2]),
        yellow: color(ansi[3]),
        orange: mix(ansi[1], ansi[3], 0.5),
        blue: color(ansi[4]),
        cyan: color(ansi[6]),
        added: mix(bg, ansi[2], tint),
        removed: mix(bg, ansi[1], tint),
        ansi: ansi.map(rgb),
        foreground_rgb: rgb(fg),
        background_rgb: rgb(bg),
        syntax: Syntax {
            keyword: color(palette.syntax[0]),
            string: color(palette.syntax[1]),
            number: color(palette.syntax[2]),
            comment: color(palette.syntax[3]),
            kind: color(palette.syntax[4]),
            function: color(palette.syntax[5]),
            constant: color(palette.syntax[6]),
            attribute: color(palette.syntax[7]),
            punctuation: mix(fg, bg, 0.3),
        },
    }
}

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
        color(if dark { on_dark } else { on_light })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_theme_overrides_its_base() {
        let dir = std::env::temp_dir().join(format!("nucli-themes-{}", std::process::id()));
        let source = std::env::temp_dir().join(format!("nucli-mint-{}.json", std::process::id()));
        std::fs::write(&source, r##"{"base": "vitesse-light", "background": "#102030"}"##).unwrap();
        let name = install(&source, &dir).unwrap();
        std::fs::write(dir.join("bad.json"), r##"{"accent": "#12345"}"##).unwrap();
        let themes = load(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_file(&source).unwrap();
        assert!(themes.error.unwrap().starts_with("themes/bad.json: invalid color"));
        let theme = resolve(&name, true, &themes.custom);
        assert!(!theme.dark);
        assert_eq!(theme.background, color(0x102030));
        assert_eq!(theme.accent, color(0x1E754F));
    }
}
