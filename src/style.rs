use crate::config::ColorMode;
use std::fmt::Write as _;

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

    pub fn start_color(&self, out: &mut String, accent: Accent) {
        if let Some(code) = self.fg_code(accent) {
            out.push_str("\x1b[");
            out.push_str(code);
            out.push('m');
        }
    }

    pub fn reset(&self, out: &mut String) {
        if self.mode != ColorMode::None {
            out.push_str("\x1b[0m");
        }
    }

    pub fn write_color(&self, out: &mut String, accent: Accent, text: &str) {
        if self.mode == ColorMode::None || text.is_empty() {
            out.push_str(text);
            return;
        }
        self.start_color(out, accent);
        out.push_str(text);
        self.reset(out);
    }

    pub fn write_bold_color(&self, out: &mut String, accent: Accent, text: &str) {
        if self.mode == ColorMode::None || text.is_empty() {
            out.push_str(text);
            return;
        }
        out.push_str("\x1b[1m");
        self.start_color(out, accent);
        out.push_str(text);
        self.reset(out);
    }

    pub fn write_dim(&self, out: &mut String, text: &str) {
        self.write_color(out, Accent::Muted, text);
    }

    pub fn write_faint(&self, out: &mut String, text: &str) {
        self.write_color(out, Accent::Faint, text);
    }

    pub fn write_bold(&self, out: &mut String, text: &str) {
        if self.mode == ColorMode::None || text.is_empty() {
            out.push_str(text);
            return;
        }
        out.push_str("\x1b[1m");
        out.push_str(text);
        self.reset(out);
    }

    pub fn color(&self, accent: Accent, text: &str) -> String {
        let mut out = String::new();
        self.write_color(&mut out, accent, text);
        out
    }

    pub fn bold(&self, text: &str) -> String {
        let mut out = String::new();
        self.write_bold(&mut out, text);
        out
    }

    pub fn dim(&self, text: &str) -> String {
        self.color(Accent::Muted, text)
    }

    pub fn faint(&self, text: &str) -> String {
        self.color(Accent::Faint, text)
    }

    pub fn bold_color(&self, accent: Accent, text: &str) -> String {
        let mut out = String::new();
        self.write_bold_color(&mut out, accent, text);
        out
    }

    fn fg_code(&self, accent: Accent) -> Option<&'static str> {
        match self.mode {
            ColorMode::None => None,
            ColorMode::TrueColor => Some(match accent {
                Accent::Cyan => "38;2;34;211;238",
                Accent::Green => "38;2;52;211;153",
                Accent::Yellow => "38;2;251;191;36",
                Accent::Red => "38;2;248;113;113",
                Accent::Muted => "38;2;201;209;217",
                Accent::Faint => "38;2;110;118;129",
            }),
            ColorMode::Ansi256 => Some(match accent {
                Accent::Cyan => "38;5;45",
                Accent::Green => "38;5;48",
                Accent::Yellow => "38;5;214",
                Accent::Red => "38;5;203",
                Accent::Muted => "38;5;252",
                Accent::Faint => "38;5;242",
            }),
            ColorMode::Ansi16 => Some(match accent {
                Accent::Cyan => "36",
                Accent::Green => "32",
                Accent::Yellow => "33",
                Accent::Red => "31",
                Accent::Muted => "37",
                Accent::Faint => "90",
            }),
        }
    }

    /// Kept for call sites that need a single styled write into a formatter.
    #[allow(dead_code)]
    pub fn write_fmt_colored(
        &self,
        out: &mut String,
        accent: Accent,
        args: std::fmt::Arguments<'_>,
    ) {
        if self.mode == ColorMode::None {
            let _ = out.write_fmt(args);
            return;
        }
        self.start_color(out, accent);
        let _ = out.write_fmt(args);
        self.reset(out);
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
