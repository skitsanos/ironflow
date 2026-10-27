use unicode_bidi::{BidiClass, Level, bidi_class};
use unicode_script::{Script, UnicodeScript};

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Auto,
    Ltr,
    Rtl,
}

impl Direction {
    pub fn level(self) -> Option<Level> {
        match self {
            Self::Auto => None,
            Self::Ltr => Some(Level::ltr()),
            Self::Rtl => Some(Level::rtl()),
        }
    }
    pub fn rtl(self, text: &str) -> bool {
        match self {
            Self::Ltr => false,
            Self::Rtl => true,
            Self::Auto => text
                .chars()
                .find_map(|c| match bidi_class(c) {
                    BidiClass::L => Some(false),
                    BidiClass::R | BidiClass::AL => Some(true),
                    _ => None,
                })
                .unwrap_or(false),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Face {
    Latin,
    Arabic,
    Hebrew,
    Cjk,
}

impl Face {
    pub fn of(c: char) -> Option<Self> {
        match c.script() {
            Script::Arabic => Some(Self::Arabic),
            Script::Hebrew => Some(Self::Hebrew),
            Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul => Some(Self::Cjk),
            Script::Common | Script::Inherited => None,
            _ => Some(Self::Latin),
        }
    }
    pub fn family(self) -> &'static str {
        match self {
            Self::Latin => "Noto Serif",
            Self::Arabic => "Noto Sans Arabic",
            Self::Hebrew => "Noto Sans Hebrew",
            Self::Cjk => "Noto Sans CJK SC",
        }
    }
    pub fn locale<'a>(self, text: &str, configured: &'a str) -> &'a str {
        match self {
            Self::Arabic => "ar",
            Self::Hebrew => "he",
            Self::Cjk
                if ["zh", "ja", "ko"]
                    .iter()
                    .any(|prefix| configured.to_ascii_lowercase().starts_with(prefix)) =>
            {
                configured
            }
            Self::Cjk if text.chars().any(|c| c.script() == Script::Hangul) => "ko",
            Self::Cjk
                if text
                    .chars()
                    .any(|c| matches!(c.script(), Script::Hiragana | Script::Katakana)) =>
            {
                "ja"
            }
            Self::Cjk => "zh-CN",
            Self::Latin => configured,
        }
    }
}
