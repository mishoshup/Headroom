use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, IsTerminal, Write};

use crate::config::{pct_to_bp, Config};
use crate::credit::{self, Applied};
use crate::pace::{Pace, BP_MAX};
use crate::state::State;
use crate::style::{Accent, Theme};
use crate::util::{duration, format_pct, unix_now};

const PRESETS: [i64; 6] = [500, 1_000, 1_500, 2_000, 2_500, 3_000];
const LABEL_WIDTH: usize = 12;

#[derive(Clone, Copy)]
enum Slot {
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
    state: State,
    base: Pace,
    now: u64,
    existing_bp: i64,
    selected: usize,
    input: Option<String>,
    error: Option<String>,
}

pub fn run(cfg: &Config) -> Result<Option<String>, String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return credit::status_text(cfg).map(Some);
    }

    let now = unix_now();
    let mut state = State::load(&cfg.state)
        .ok_or("no usage snapshot yet; start Claude Code once so the statusline can capture it")?;
    crate::pace::refresh_state(&mut state, cfg.week_days, now);
    let base = Pace::compute(cfg, &state, now, 0)
        .ok_or("the weekly window is not available in the snapshot; run headroom doctor")?;
    let (existing_bp, _) = credit::active_bp(cfg, now, base.window_end, base.day);

    let mut panel = Panel {
        cfg,
        theme: Theme::new(cfg.color, cfg.ascii),
        state,
        base,
        now,
        existing_bp,
        selected: 0,
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
                KeyCode::Char('c') => return self.clear(),
                KeyCode::Enter => match self.selected_slot() {
                    Some(Slot::Preset(bp)) => {
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
        if self.max_add_bp() > 0 {
            for preset in PRESETS {
                slots.push(Slot::Preset(preset));
            }
            slots.push(Slot::Custom);
        }
        slots.push(Slot::Clear);
        slots.push(Slot::Quit);
        slots
    }

    fn selected_slot(&self) -> Option<Slot> {
        self.slots().get(self.selected).copied()
    }

    fn max_add_bp(&self) -> i64 {
        (BP_MAX - self.base.base_cap_bp - self.existing_bp).max(0)
    }

    fn ceiling_bp(&self) -> i64 {
        (self.base.base_cap_bp + self.existing_bp).clamp(0, BP_MAX)
    }

    fn left_bp(&self) -> i64 {
        (self.ceiling_bp() - self.base.used_bp).max(0)
    }

    fn outcome(&self, add_bp: i64) -> (i64, i64) {
        let ceiling = (self.ceiling_bp() + add_bp).clamp(0, BP_MAX);
        (ceiling, (ceiling - self.base.used_bp).max(0))
    }

    fn borrow(&mut self, add_bp: i64) -> Result<Option<String>, String> {
        match credit::apply(self.cfg, &self.state, self.now, add_bp, None) {
            Ok(applied) => Ok(Some(self.applied_message(applied))),
            Err(err) => {
                self.error = Some(err);
                Ok(None)
            }
        }
    }

    fn clear(&mut self) -> Result<Option<String>, String> {
        credit::Credit::clear(&self.cfg.credit)
            .map_err(|e| format!("could not clear the borrow: {e}"))?;
        Ok(Some(
            "headroom: borrow cleared; today's normal allowance applies again.".into(),
        ))
    }

    fn applied_message(&self, applied: Applied) -> String {
        let mut out = format!(
            "headroom: added +{}% for today",
            format_pct(applied.added_bp)
        );
        if applied.total_bp != applied.added_bp {
            out.push_str(&format!(
                " (total +{}% borrowed)",
                format_pct(applied.total_bp)
            ));
        }
        out.push_str(&format!(
            ".\n{}% left today.\n",
            format_pct(applied.left_bp)
        ));
        if applied.days_after > 0 {
            out.push_str(&format!(
                "next {} days {}%/day.\n",
                applied.days_after,
                format_pct(applied.future_per_day_bp)
            ));
        } else {
            out.push_str("last day of the week.\n");
        }
        out.push_str(&format!(
            "expires in {}.",
            duration(applied.expires_at - self.now as i64)
        ));
        out
    }

    fn input_bp(&self) -> Option<i64> {
        self.input
            .as_deref()
            .and_then(pct_to_bp)
            .filter(|bp| *bp > 0 && *bp <= BP_MAX)
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
        let left = self.left_bp();
        let used = self.base.used_bp;

        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "{}  {}",
            self.theme.bold_color(Accent::Cyan, "Headroom"),
            self.theme.dim("borrow from the rest of the week")
        ));

        let verdict = if left > 0 {
            self.theme.color(
                Accent::Green,
                &format!("You have {}% left today.", format_pct(left)),
            )
        } else {
            self.theme.color(Accent::Red, "Today's allowance is used.")
        };
        lines.push(format!(
            "{verdict}  {}",
            self.theme
                .dim(&format!("Weekly usage is {}%.", format_pct(used)))
        ));
        lines.push(self.theme.faint(&rule));

        lines.push(self.metric_row(
            "weekly used",
            used,
            format!("{}%", format_pct(used)),
            Some(format!(
                "resets in {}",
                duration(
                    self.state
                        .seven_day
                        .and_then(|window| window.resets_at)
                        .map(|reset| reset - self.now as i64)
                        .unwrap_or(0)
                )
            )),
            None,
            cells,
        ));
        let ratio = if self.ceiling_bp() > 0 {
            used.saturating_mul(10_000) / self.ceiling_bp()
        } else {
            0
        };
        lines.push(self.metric_row(
            "today",
            ratio,
            format!("{}%", format_pct(left)),
            Some("left".into()),
            Some(if left == 0 {
                Accent::Red
            } else {
                Accent::Green
            }),
            cells,
        ));

        let next = if self.base.days_after > 0 {
            format!(
                "{} days · {}%/day",
                self.base.days_after,
                format_pct(self.base.future_per_day_bp)
            )
        } else {
            "last day of the week".into()
        };
        let mut next_line = format!(
            "{}  {}",
            self.theme.dim(&format!("{:<LABEL_WIDTH$}", "next days")),
            self.theme.dim(&next)
        );
        if self.existing_bp > 0 {
            next_line.push_str(&format!(
                " {} {}",
                self.theme.faint("·"),
                self.theme.color(
                    Accent::Cyan,
                    &format!("+{}% borrowed", format_pct(self.existing_bp))
                )
            ));
        }
        lines.push(next_line);

        lines.push(String::new());
        let title = if self.max_add_bp() > 0 {
            "Borrow from the rest of the week"
        } else {
            "No future budget left to borrow"
        };
        lines.push(self.theme.bold(title));

        if let Some(input) = &self.input {
            lines.push(format!(
                "{}  {}",
                self.theme.dim("Borrow percent:"),
                self.theme.bold_color(
                    Accent::Cyan,
                    &format!("{}%", if input.is_empty() { "…" } else { input })
                )
            ));
        } else {
            for (index, slot) in self.slots().iter().enumerate() {
                lines.push(match slot {
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
        } else if self.input.is_some() {
            if let Some(bp) = self.input_bp() {
                lines.push(self.preview(bp));
            } else {
                lines.push(self.theme.dim("Type a percentage between 0 and 100."));
            }
        } else if let Some(Slot::Preset(bp)) = self.selected_slot() {
            lines.push(self.preview(bp));
        } else if let Some(Slot::Clear) = self.selected_slot() {
            lines.push(
                self.theme
                    .dim("Removes the borrow; today's normal allowance applies again."),
            );
        }

        lines.push(String::new());
        let footer = if self.input.is_some() {
            "enter confirm · esc cancel"
        } else if self.theme.ascii() {
            "j/k choose · enter confirm · c clear · q quit"
        } else {
            "↑/↓ choose · enter confirm · c clear · q quit"
        };
        lines.push(self.theme.dim(footer));

        queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;
        for line in lines {
            write!(out, " {line}\r\n")?;
        }
        out.flush()
    }

    fn preview(&self, add_bp: i64) -> String {
        let (_, left) = self.outcome(add_bp);
        let next = if self.base.days_after > 0 {
            format!(
                "next {} days {}%/day",
                self.base.days_after,
                format_pct(self.base.future_per_day_bp)
            )
        } else {
            "last day".into()
        };
        format!(
            "add +{}%  →  {}% left today  ·  {next}  ·  expires in {}",
            format_pct(add_bp),
            format_pct(left),
            duration(self.base.next_epoch - self.now as i64)
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

    fn preset_row(&self, index: usize, add_bp: i64) -> String {
        let selected = self.selected == index;
        let (_, left) = self.outcome(add_bp);
        let detail = format!("today {}% left", format_pct(left));
        let label = format!("+{}%", format_pct(add_bp));
        if selected {
            format!(
                "{} {}  {}",
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
