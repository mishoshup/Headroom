use crate::config::ColorMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    Cyan,
    Green,
    Yellow,
    Red,
    Muted,
    Faint,
}

pub struct Theme {
    mode: ColorMode,
    ascii: bool,
}

impl Theme {
    pub fn new(mode: ColorMode, ascii: bool) -> Self {
        Self { mode, ascii }
    }

    pub fn ascii(&self) -> bool {
        self.ascii
    }

    pub fn color(&self, accent: Accent, text: &str) -> String {
        match self.fg_code(accent) {
            Some(code) if !text.is_empty() => format!("\x1b[{code}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }

    pub fn bold(&self, text: &str) -> String {
        self.wrap("1", text)
    }

    pub fn dim(&self, text: &str) -> String {
        self.color(Accent::Muted, text)
    }

    pub fn faint(&self, text: &str) -> String {
        self.color(Accent::Faint, text)
    }

    pub fn bold_color(&self, accent: Accent, text: &str) -> String {
        match self.fg_code(accent) {
            Some(code) if !text.is_empty() => {
                format!("\x1b[1m\x1b[{code}m{text}\x1b[0m")
            }
            _ => text.to_string(),
        }
    }

    fn wrap(&self, code: &str, text: &str) -> String {
        if self.mode == ColorMode::None || text.is_empty() {
            text.to_string()
        } else {
            format!("\x1b[{code}m{text}\x1b[0m")
        }
    }

    fn fg_code(&self, accent: Accent) -> Option<String> {
        match self.mode {
            ColorMode::None => None,
            ColorMode::TrueColor => {
                let (r, g, b) = rgb(accent);
                Some(format!("38;2;{r};{g};{b}"))
            }
            ColorMode::Ansi256 => Some(format!("38;5;{}", ansi256(accent))),
            ColorMode::Ansi16 => Some(format!("{}", ansi16(accent))),
        }
    }
}

fn rgb(accent: Accent) -> (u8, u8, u8) {
    match accent {
        Accent::Cyan => (34, 211, 238),
        Accent::Green => (52, 211, 153),
        Accent::Yellow => (251, 191, 36),
        Accent::Red => (248, 113, 113),
        Accent::Muted => (201, 209, 217),
        Accent::Faint => (110, 118, 129),
    }
}

fn ansi256(accent: Accent) -> u8 {
    match accent {
        Accent::Cyan => 45,
        Accent::Green => 48,
        Accent::Yellow => 214,
        Accent::Red => 203,
        Accent::Muted => 252,
        Accent::Faint => 242,
    }
}

fn ansi16(accent: Accent) -> u8 {
    match accent {
        Accent::Cyan => 36,
        Accent::Green => 32,
        Accent::Yellow => 33,
        Accent::Red => 31,
        Accent::Muted => 37,
        Accent::Faint => 90,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_mode_emits_plain_text() {
        let t = Theme::new(ColorMode::None, false);
        assert_eq!(t.color(Accent::Cyan, "hi"), "hi");
        assert_eq!(t.bold("hi"), "hi");
        assert_eq!(t.dim("hi"), "hi");
        assert_eq!(t.faint("hi"), "hi");
    }

    #[test]
    fn muted_and_faint_are_light_and_dark_greys() {
        let t = Theme::new(ColorMode::TrueColor, false);
        assert_eq!(t.dim("hi"), "\x1b[38;2;201;209;217mhi\x1b[0m");
        assert_eq!(t.faint("hi"), "\x1b[38;2;110;118;129mhi\x1b[0m");
    }

    #[test]
    fn truecolor_emits_rgb_sgr() {
        let t = Theme::new(ColorMode::TrueColor, false);
        assert_eq!(
            t.color(Accent::Green, "hi"),
            "\x1b[38;2;52;211;153mhi\x1b[0m"
        );
    }

    #[test]
    fn ansi16_emits_basic_sgr() {
        let t = Theme::new(ColorMode::Ansi16, false);
        assert_eq!(t.color(Accent::Red, "hi"), "\x1b[31mhi\x1b[0m");
    }
}
