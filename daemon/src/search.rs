//! Search in the history of a session (Ctrl+Shift+F): every match of a text, smart case (an upper-case letter makes it
//! case sensitive), with one of them current. The matches live here, next to the grid; the window only gets the ones
//! on screen, in each frame, and the count.

use std::time::{Duration, Instant};

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Point};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::Term;

/// Matches kept (a search for "e" in a long history stops here)
const LIMIT: usize = 20_000;
/// Shortest time between two searches while output arrives
pub const REFRESH: Duration = Duration::from_millis(200);

pub struct Search {
    regex: RegexSearch,
    pub matches: Vec<Match>,
    /// Index into `matches` of the current match
    pub current: usize,
    pub searched_at: Instant,
    /// Output arrived since `matches` were found
    pub stale: bool,
}

/// The text as a regular expression that matches it literally
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl Search {
    pub fn new<T>(term: &Term<T>, query: &str) -> Option<Self> {
        if query.is_empty() {
            return None;
        }
        let regex = RegexSearch::new(&escape(query)).ok()?;
        let mut s = Self { regex, matches: Vec::new(), current: 0, searched_at: Instant::now(), stale: false };
        s.run(term);
        // The match nearest to the bottom of what is on screen
        let bottom = term.screen_lines() as i32 - 1 - term.grid().display_offset() as i32;
        s.current = s.matches.iter().rposition(|m| m.start().line.0 <= bottom).unwrap_or(0);
        Some(s)
    }

    /// Find every match again (new output moved the history)
    pub fn run<T>(&mut self, term: &Term<T>) {
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        self.matches = RegexIter::new(start, end, Direction::Right, term, &mut self.regex).take(LIMIT).collect();
        self.current = self.current.min(self.matches.len().saturating_sub(1));
        self.searched_at = Instant::now();
        self.stale = false;
    }

    /// Next (1) or previous (-1) match, wrapping around
    pub fn step(&mut self, direction: i32) {
        let n = self.matches.len();
        if n > 0 {
            self.current = if direction < 0 { (self.current + n - 1) % n } else { (self.current + 1) % n };
        }
    }

    /// 1-based position of the current match (0: no match)
    pub fn position(&self) -> u32 {
        if self.matches.is_empty() { 0 } else { self.current as u32 + 1 }
    }

    /// Scroll the view so the current match is on screen (a third of the way down when it was not)
    pub fn reveal<T: EventListener>(&self, term: &mut Term<T>) {
        let Some(m) = self.matches.get(self.current) else { return };
        let rows = term.screen_lines() as i32;
        let offset = term.grid().display_offset() as i32;
        let line = m.start().line.0;
        let row = line + offset;
        if (0..rows).contains(&row) {
            return;
        }
        let wanted = (rows / 3 - line).max(0);
        term.scroll_display(Scroll::Delta(wanted - offset));
    }

    /// The matches on screen as (row, first column, last column, current), a match on several rows being cut per row
    pub fn visible<T>(&self, term: &Term<T>, offset: i32) -> Vec<(u16, u16, u16, bool)> {
        let rows = term.screen_lines() as i32;
        let last = term.columns() as i32 - 1;
        let (top, bottom) = (-offset, rows - 1 - offset);
        // Matches are in grid order: skip those above the view, stop after it
        let first = self.matches.partition_point(|m| m.end().line.0 < top);
        let mut out = Vec::new();
        for (i, m) in self.matches.iter().enumerate().skip(first) {
            let (a, b) = (m.start(), m.end());
            if a.line.0 > bottom {
                break;
            }
            for line in a.line.0.max(top)..=b.line.0.min(bottom) {
                let from = if line == a.line.0 { a.column.0 as i32 } else { 0 };
                let to = if line == b.line.0 { b.column.0 as i32 } else { last };
                out.push(((line + offset) as u16, from as u16, to.min(last) as u16, i == self.current));
            }
        }
        out
    }
}
