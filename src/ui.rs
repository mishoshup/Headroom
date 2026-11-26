use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, IsTerminal, Write};

use crate::config::{pct_to_bp, Config};
use crate::credit::{self, Applied, Scope};
use crate::pace::Pace;
use crate::state::State;
use crate::style::{Accent, Theme};
use crate::util::{duration, format_pct, unix_now};

const PRESETS: [i64; 6] = [500, 1_000, 1_500, 2_000, 2_500, 3_000];
const LABEL_WIDTH: usize = 12;

#[derive(Clone, Copy)]
enum Slot {
    Fix(i64),
    Preset(i64),
    Custom,
    Clear,
    Quit,
}

struct Screen;

impl Screen {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(err);
        }
        Ok(Self)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

struct Panel<'a> {
    cfg: &'a Config,
    theme: Theme,
    base: Pace,
    reset: i64,
    used_bp: i64,
    existing_bp: i64,
    existing_scope: Option<Scope>,
    now: u64,
    selected: usize,
    week: bool,
    input: Option<String>,
    error: Option<String>,
}

pub fn run(cfg: &Config) -> Result<Option<String>, String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return credit::status_text(cfg).map(Some);
    }

    let now = unix_now();
    let state = State::load(&cfg.state)
        .ok_or("no usage snapshot yet; start Claude Code once so the statusline can capture it")?;
    let week = state
        .seven_day
        .ok_or("the weekly window is not exposed for this account")?;
    let reset = week
        .resets_at
        .ok_or("the snapshot has no weekly reset time")?;
    let base = Pace::compute(cfg, week.used_bp, reset, now, 0)
        .ok_or("the weekly window has already rolled over; wait for the next statusline tick")?;
    let (existing_bp, existing) = credit::active_bp(cfg, now, reset, base.day);
    let existing_scope = existing.map(|c| c.scope);

    let mut panel = Panel {
        cfg,
        theme: Theme::new(cfg.color, cfg.ascii),
        base,
        reset,
        used_bp: week.used_bp,
        existing_bp,
        existing_scope,
        now,
        selected: 0,
        week: existing_scope == Some(Scope::Week),
        input: None,
        error: None,
    };

    let screen = Screen::enter().map_err(|e| format!("could not start the panel: {e}"))?;
    let result = panel.run();
    drop(screen);
    result
}

impl Panel<'_> {
    fn run(&mut self) -> Result<Option<String>, String> {
        let mut out = io::stdout();
        loop {
            self.draw(&mut out)
                .map_err(|e| format!("could not draw the panel: {e}"))?;
            let event = event::read().map_err(|e| format!("could not read input: {e}"))?;
            let Event::Key(key) = event else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(None);
            }

            if self.input.is_some() {
                match key.code {
                    KeyCode::Esc => {
                        self.input = None;
                        self.error = None;
                    }
                    KeyCode::Backspace => {
                        if let Some(input) = self.input.as_mut() {
                            input.pop();
                        }
                        self.error = None;
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                        if let Some(input) = self.input.as_mut() {
                            if input.len() < 6 {
                                input.push(c);
                            }
                        }
                        self.error = None;
                    }
                    KeyCode::Enter => {
                        if let Some(bp) = self.input_bp() {
                            if let Some(message) = self.borrow(bp)? {
                                return Ok(Some(message));
                            }
                        } else {
                            self.error = Some("Enter a number between 0 and 100.".into());
                        }
                    }
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Up | KeyCode::Char('k') => {
                    let count = self.slots().len();
                    self.selected = (self.selected + count - 1) % count;
                    self.error = None;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let count = self.slots().len();
                    self.selected = (self.selected + 1) % count;
                    self.error = None;
                }
                KeyCode::Char('w') => {
                    self.week = !self.week;
                    self.error = None;
                }
                KeyCode::Char('c') => return self.clear(),
                KeyCode::Enter => match self.selected_slot() {
                    Some(Slot::Fix(bp)) | Some(Slot::Preset(bp)) => {
                        if let Some(message) = self.borrow(bp)? {
                            return Ok(Some(message));
                        }
                    }
                    Some(Slot::Custom) => {
                        self.input = Some(String::new());
                        self.error = None;
                    }
                    Some(Slot::Clear) => return self.clear(),
                    Some(Slot::Quit) | None => return Ok(None),
                },
                _ => {}
            }
        }
    }

    fn slots(&self) -> Vec<Slot> {
        let mut slots = Vec::new();
        let fix = self.fix_bp();
        if let Some(fix) = fix {
            slots.push(Slot::Fix(fix));
        }
        for preset in PRESETS {
            if fix == Some(preset) {
                continue;
            }
            slots.push(Slot::Preset(preset));
        }
        slots.push(Slot::Custom);
        slots.push(Slot::Clear);
        slots.push(Slot::Quit);
        slots
    }

    fn selected_slot(&self) -> Option<Slot> {
        self.slots().get(self.selected).copied()
    }

    fn ceiling_bp(&self) -> i64 {
        (self.base.ceiling_bp + self.existing_bp).clamp(0, 10_000)
    }

    fn over_bp(&self) -> i64 {
        (self.used_bp - self.ceiling_bp()).max(0)
    }

    /// Smallest whole-percent borrow that gets back under, plus one point of
    /// headroom. Capped at the remaining weekly room.
    fn fix_bp(&self) -> Option<i64> {
        let over = self.over_bp();
        if over <= 0 {
            return None;
        }
        let room = 10_000 - self.existing_bp;
        if room <= 0 {
            return None;
        }
        let whole = ((over + 99) / 100) * 100;
        Some((whole + 100).clamp(1, room))
    }

    fn outcome(&self, add_bp: i64) -> (i64, i64) {
        let total = (self.existing_bp + add_bp).clamp(0, 10_000);
        let limit = (self.base.ceiling_bp + total).clamp(0, 10_000);
        (limit, self.used_bp - limit)
    }

    fn borrow(&mut self, add_bp: i64) -> Result<Option<String>, String> {
        match credit::apply(
            self.cfg, self.now, self.reset, &self.base, add_bp, self.week, None,
        ) {
            Ok(applied) => Ok(Some(self.applied_message(applied))),
            Err(err) => {
                self.error = Some(err);
                Ok(None)
            }
        }
    }

    fn clear(&mut self) -> Result<Option<String>, String> {
        credit::Credit::clear(&self.cfg.credit)
            .map_err(|e| format!("could not clear the override: {e}"))?;
        Ok(Some(
            "usage-pacer: override cleared; the normal daily pace applies again.".into(),
        ))
    }

    fn applied_message(&self, applied: Applied) -> String {
        let previous = self.ceiling_bp();
        let delta = self.used_bp - applied.ceiling_bp;
        let scope_text = match applied.scope {
            Scope::Week => "the rest of the weekly window",
            Scope::Day => "today",
        };
        let mut out = format!(
            "usage-pacer: added +{}% for {scope_text}",
            format_pct(applied.added_bp)
        );
        if applied.total_bp != applied.added_bp {
            out.push_str(&format!(
                " (total +{}% borrowed).\n",
                format_pct(applied.total_bp)
            ));
        } else {
            out.push_str(".\n");
        }
        out.push_str(&format!(
            "today's limit: {}% -> {}%.\n",
            format_pct(previous),
            format_pct(applied.ceiling_bp)
        ));
        if delta > 0 {
            out.push_str(&format!(
                "You are still {}% over today.\n",
                format_pct(delta)
            ));
        } else {
            out.push_str(&format!(
                "You now have {}% left today.\n",
                format_pct(-delta)
            ));
        }
        out.push_str(&format!(
            "Expires in {}.",
            duration(applied.expires_at - self.now as i64)
        ));
        out
    }

    fn input_bp(&self) -> Option<i64> {
        self.input
            .as_deref()
            .and_then(pct_to_bp)
            .filter(|bp| *bp > 0 && *bp <= 10_000)
    }

    fn effective_week(&self) -> bool {
        self.week || self.existing_scope == Some(Scope::Week)
    }

    fn expiry(&self) -> i64 {
        if self.effective_week() {
            self.reset
        } else {
            self.base.next_epoch
        }
    }

    fn section_title(&self) -> String {
        if self.over_bp() > 0 {
            "Borrow to get back under today's limit".into()
        } else if self.week {
            "Borrow extra for the rest of the week".into()
        } else {
            "Borrow extra for today".into()
        }
    }

    fn draw(&self, out: &mut impl Write) -> io::Result<()> {
        let width = terminal::size()
            .map(|(cols, _)| cols as usize)
            .unwrap_or(80)
            .clamp(40, 80);
        let rule = if self.theme.ascii() {
            "-".repeat(width.saturating_sub(2))
        } else {
            "─".repeat(width.saturating_sub(2))
        };
        let cells = width.saturating_sub(38).clamp(12, 20);
        let delta = self.used_bp - self.ceiling_bp();

        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "{}  {}",
            self.theme.bold_color(Accent::Cyan, "usage-pacer"),
            self.theme.dim("borrow extra headroom")
        ));

        let mut verdict = if delta > 0 {
            self.theme.color(
                Accent::Red,
                &format!("You are {}% over today's limit.", format_pct(delta)),
            )
        } else {
            self.theme.color(
                Accent::Green,
                &format!("You have {}% left today.", format_pct(-delta)),
            )
        };
        verdict.push_str(
            &self
                .theme
                .dim(&format!("  Weekly usage is {}%.", format_pct(self.used_bp))),
        );
        lines.push(verdict);
        lines.push(self.theme.faint(&rule));

        lines.push(self.metric_row(
            "weekly used",
            self.used_bp,
            format!("{}%", format_pct(self.used_bp)),
            Some(format!(
                "resets in {}",
                duration(self.reset - self.now as i64)
            )),
            None,
            cells,
        ));
        let ratio =
            (self.used_bp.saturating_mul(10_000) / self.ceiling_bp().max(1)).clamp(0, 10_000);
        let today_detail = if delta > 0 {
            format!("{}% over", format_pct(delta))
        } else {
            format!("{}% left", format_pct(-delta))
        };
        lines.push(self.metric_row(
            "today limit",
            ratio,
            format!("{}%", format_pct(self.ceiling_bp())),
            Some(today_detail),
            Some(if delta > 0 {
                Accent::Red
            } else {
                Accent::Green
            }),
            cells,
        ));

        if self.existing_bp > 0 {
            let scope = if self.effective_week() {
                "rest of week"
            } else {
                "today"
            };
            lines.push(format!(
                "{}  {}  {}",
                self.theme.dim(&format!("{:<LABEL_WIDTH$}", "borrowed")),
                self.theme
                    .color(Accent::Cyan, &format!("+{}%", format_pct(self.existing_bp))),
                self.theme.dim(&format!(
                    "for {scope}, expires in {}",
                    duration(self.expiry() - self.now as i64)
                ))
            ));
        }

        lines.push(String::new());
        lines.push(self.theme.bold(&self.section_title()));

        if let Some(input) = &self.input {
            lines.push(format!(
                "{}  {}",
                self.theme.dim("Extra percent:"),
                self.theme.bold_color(
                    Accent::Cyan,
                    &format!("{}%", if input.is_empty() { "…" } else { input })
                )
            ));
        } else {
            for (index, slot) in self.slots().iter().enumerate() {
                lines.push(match slot {
                    Slot::Fix(bp) => self.fix_row(index, *bp),
                    Slot::Preset(bp) => self.preset_row(index, *bp),
                    Slot::Custom => self.simple_row(index, "custom amount…", false),
                    Slot::Clear => {
                        self.simple_row(index, "clear current borrow", self.existing_bp == 0)
                    }
                    Slot::Quit => self.simple_row(index, "quit", false),
                });
            }
        }

        lines.push(String::new());
        if let Some(error) = &self.error {
            lines.push(self.theme.color(Accent::Red, error));
        } else if let Some(bp) = self.input.as_ref().and_then(|_| self.input_bp()) {
            lines.push(self.preview(bp));
        } else if self.input.is_some() {
            lines.push(
                self.theme
                    .dim("Type how many extra points you want, then press enter."),
            );
        } else if let Some(slot) = self.selected_slot() {
            match slot {
                Slot::Fix(bp) | Slot::Preset(bp) => lines.push(self.preview(bp)),
                Slot::Clear => lines.push(
                    self.theme
                        .dim("Removes the current borrow; the normal daily limit applies again."),
                ),
                _ => {}
            }
        }

        lines.push(String::new());
        let footer = if self.input.is_some() {
            "enter confirm · esc cancel"
        } else if self.theme.ascii() {
            "j/k choose · enter confirm · w scope · c clear · q quit"
        } else {
            "↑/↓ choose · enter confirm · w scope · c clear · q quit"
        };
        lines.push(self.theme.dim(footer));

        queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;
        for line in lines {
            write!(out, " {line}\r\n")?;
        }
        out.flush()
    }

    fn preview(&self, add_bp: i64) -> String {
        let (limit, delta) = self.outcome(add_bp);
        let tail = if delta > 0 {
            self.theme
                .color(Accent::Red, &format!("{}% over", format_pct(delta)))
        } else {
            self.theme
                .color(Accent::Green, &format!("{}% left", format_pct(-delta)))
        };
        format!(
            "Selected: add +{}%  →  limit {}%  ·  {tail}  ·  expires in {}",
            format_pct(add_bp),
            format_pct(limit),
            duration(self.expiry() - self.now as i64)
        )
    }

    fn metric_row(
        &self,
        label: &str,
        bar_bp: i64,
        value: String,
        detail: Option<String>,
        detail_accent: Option<Accent>,
        cells: usize,
    ) -> String {
        let mut out = self.theme.dim(&format!("{label:<LABEL_WIDTH$}"));
        out.push_str(&bar(bar_bp, cells, &self.theme));
        out.push_str("  ");
        out.push_str(&self.theme.bold(&format!("{value:>5}")));
        if let Some(detail) = detail {
            let styled = match detail_accent {
                Some(accent) => self.theme.color(accent, &detail),
                None => self.theme.dim(&detail),
            };
            out.push_str(&format!("  {styled}"));
        }
        out
    }

    fn fix_row(&self, index: usize, add_bp: i64) -> String {
        let selected = self.selected == index;
        let (limit, delta) = self.outcome(add_bp);
        let tag = if delta <= 0 {
            "gets you under"
        } else {
            "largest available"
        };
        let detail = if delta > 0 {
            format!(
                "→ limit {}% · still {}% over",
                format_pct(limit),
                format_pct(delta)
            )
        } else {
            format!(
                "→ limit {}% · {}% left",
                format_pct(limit),
                format_pct(-delta)
            )
        };
        let label = format!("+{}%", format_pct(add_bp));
        if selected {
            format!(
                "{} {}   {}   {}",
                self.theme.color(Accent::Cyan, "▸"),
                self.theme.bold_color(Accent::Cyan, &format!("{label:<6}")),
                self.theme.color(Accent::Yellow, tag),
                self.theme.dim(&detail)
            )
        } else {
            format!(
                "  {label:<6}  {}  {}",
                self.theme.color(Accent::Yellow, tag),
                self.theme.dim(&detail)
            )
        }
    }

    fn preset_row(&self, index: usize, add_bp: i64) -> String {
        let selected = self.selected == index;
        let (limit, delta) = self.outcome(add_bp);
        let detail = if delta > 0 {
            format!(
                "→ limit {}% · still {}% over",
                format_pct(limit),
                format_pct(delta)
            )
        } else {
            format!(
                "→ limit {}% · {}% left",
                format_pct(limit),
                format_pct(-delta)
            )
        };
        let label = format!("+{}%", format_pct(add_bp));
        if selected {
            format!(
                "{} {}   {}",
                self.theme.color(Accent::Cyan, "▸"),
                self.theme.bold_color(Accent::Cyan, &format!("{label:<6}")),
                self.theme.dim(&detail)
            )
        } else {
            format!("  {label:<6}  {}", self.theme.dim(&detail))
        }
    }

    fn simple_row(&self, index: usize, label: &str, disabled: bool) -> String {
        let selected = self.selected == index;
        if selected && !disabled {
            format!(
                "{} {}",
                self.theme.color(Accent::Cyan, "▸"),
                self.theme.bold_color(Accent::Cyan, label)
            )
        } else {
            format!("  {}", self.theme.dim(label))
        }
    }
}

fn bar(bp: i64, cells: usize, theme: &Theme) -> String {
    let bp = bp.clamp(0, 10_000);
    let filled = ((bp * cells as i64 + 5_000) / 10_000) as usize;
    let (full, empty) = if theme.ascii() {
        ('#', '-')
    } else {
        ('▰', '▱')
    };
    let color = if bp >= 8_000 {
        Accent::Red
    } else if bp >= 6_000 {
        Accent::Yellow
    } else {
        Accent::Green
    };
    let fill = full.to_string().repeat(filled);
    let pad = empty.to_string().repeat(cells - filled);
    format!("{}{}", theme.color(color, &fill), theme.faint(&pad))
}
