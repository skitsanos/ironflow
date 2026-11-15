//! Lua 5.4 pattern matching with a step budget (IF-213).
//!
//! A port of the matcher in the reference `lstrlib.c`, kept structurally
//! close to it so the two can be compared. The reference matcher backtracks
//! without any bound and cannot be interrupted; this one counts a step per
//! pattern item it tries and stops at the budget, the step deadline or
//! cancellation.

use mlua::prelude::*;

use super::budget::Meter;
use super::classes::{ESC, class_end, matches_set, single_match};

/// Captures one pattern may hold (`LUA_MAXCAPTURES`).
const MAX_CAPTURES: usize = 32;
/// Nested matcher calls one match may need (`MAXCCALLS`).
const MAX_DEPTH: u32 = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureLen {
    Unfinished,
    Position,
    Closed(usize),
}

#[derive(Clone, Copy)]
struct Capture {
    start: usize,
    len: CaptureLen,
}

/// One capture of a successful match.
pub(super) enum Captured {
    /// A substring of the subject: `start..end`.
    Text(usize, usize),
    /// A position capture `()`, one-based.
    Position(usize),
}

pub(super) fn pattern_error(message: impl Into<String>) -> LuaError {
    LuaError::runtime(message.into())
}

/// Matching state for one subject and pattern.
pub(super) struct Matcher<'a> {
    src: &'a [u8],
    pat: &'a [u8],
    level: usize,
    captures: [Capture; MAX_CAPTURES],
    depth: u32,
    pub(super) meter: Meter,
}

impl<'a> Matcher<'a> {
    pub(super) fn new(src: &'a [u8], pat: &'a [u8], meter: Meter) -> Self {
        Self {
            src,
            pat,
            level: 0,
            captures: [Capture {
                start: 0,
                len: CaptureLen::Unfinished,
            }; MAX_CAPTURES],
            depth: MAX_DEPTH,
            meter,
        }
    }

    /// Try to match the pattern from `p` at subject offset `s`; returns the
    /// offset after the match.
    pub(super) fn match_at(&mut self, s: usize, p: usize) -> LuaResult<Option<usize>> {
        self.level = 0;
        self.depth = MAX_DEPTH;
        self.do_match(s, p)
    }

    /// Values a successful match of `s..e` yields: its captures, or the whole
    /// match when the pattern has none and `whole` is set.
    pub(super) fn captured(&self, s: usize, e: usize, whole: bool) -> LuaResult<Vec<Captured>> {
        let count = if self.level == 0 && whole {
            1
        } else {
            self.level
        };
        (0..count).map(|index| self.capture(index, s, e)).collect()
    }

    /// Capture `index` of a match of `s..e`; index 0 of a pattern without
    /// captures is the whole match.
    pub(super) fn capture(&self, index: usize, s: usize, e: usize) -> LuaResult<Captured> {
        if index >= self.level {
            if index != 0 {
                return Err(pattern_error(format!(
                    "invalid capture index %{}",
                    index + 1
                )));
            }
            return Ok(Captured::Text(s, e));
        }
        let capture = self.captures[index];
        match capture.len {
            CaptureLen::Unfinished => Err(pattern_error("unfinished capture")),
            CaptureLen::Position => Ok(Captured::Position(capture.start + 1)),
            CaptureLen::Closed(len) => Ok(Captured::Text(capture.start, capture.start + len)),
        }
    }

    fn do_match(&mut self, s: usize, p: usize) -> LuaResult<Option<usize>> {
        if self.depth == 0 {
            return Err(pattern_error("pattern too complex"));
        }
        self.depth -= 1;
        let outcome = self.match_items(s, p);
        self.depth += 1;
        outcome
    }

    #[allow(clippy::cognitive_complexity)] // One dispatch over pattern items, mirroring the reference `match`.
    fn match_items(&mut self, mut s: usize, mut p: usize) -> LuaResult<Option<usize>> {
        loop {
            self.meter.step()?;
            let Some(&item) = self.pat.get(p) else {
                return Ok(Some(s));
            };
            let next = self.pat.get(p + 1).copied();
            match (item, next) {
                (b'(', Some(b')')) => return self.start_capture(s, p + 2, CaptureLen::Position),
                (b'(', _) => return self.start_capture(s, p + 1, CaptureLen::Unfinished),
                (b')', _) => return self.end_capture(s, p + 1),
                (b'$', None) => return Ok((s == self.src.len()).then_some(s)),
                (ESC, Some(b'b')) => match self.match_balance(s, p + 2)? {
                    Some(end) => (s, p) = (end, p + 4),
                    None => return Ok(None),
                },
                (ESC, Some(b'f')) => match self.match_frontier(s, p + 2)? {
                    Some(after) => p = after,
                    None => return Ok(None),
                },
                (ESC, Some(digit)) if digit.is_ascii_digit() => {
                    match self.match_capture(s, digit)? {
                        Some(end) => (s, p) = (end, p + 2),
                        None => return Ok(None),
                    }
                }
                _ => {
                    let ep = class_end(self.pat, p)?;
                    let quantifier = self.pat.get(ep).copied();
                    if !single_match(self.src, self.pat, s, p, ep) {
                        if matches!(quantifier, Some(b'*' | b'?' | b'-')) {
                            p = ep + 1;
                            continue;
                        }
                        return Ok(None);
                    }
                    match quantifier {
                        Some(b'?') => match self.do_match(s + 1, ep + 1)? {
                            Some(end) => return Ok(Some(end)),
                            None => p = ep + 1,
                        },
                        Some(b'+') => return self.max_expand(s + 1, p, ep),
                        Some(b'*') => return self.max_expand(s, p, ep),
                        Some(b'-') => return self.min_expand(s, p, ep),
                        _ => (s, p) = (s + 1, ep),
                    }
                }
            }
        }
    }

    fn max_expand(&mut self, s: usize, p: usize, ep: usize) -> LuaResult<Option<usize>> {
        let mut count = 0;
        while single_match(self.src, self.pat, s + count, p, ep) {
            count += 1;
            self.meter.step()?;
        }
        loop {
            if let Some(end) = self.do_match(s + count, ep + 1)? {
                return Ok(Some(end));
            }
            if count == 0 {
                return Ok(None);
            }
            count -= 1;
        }
    }

    fn min_expand(&mut self, mut s: usize, p: usize, ep: usize) -> LuaResult<Option<usize>> {
        loop {
            if let Some(end) = self.do_match(s, ep + 1)? {
                return Ok(Some(end));
            }
            if !single_match(self.src, self.pat, s, p, ep) {
                return Ok(None);
            }
            s += 1;
        }
    }

    /// `%bxy` at pattern offset `p` (the `x`).
    fn match_balance(&mut self, s: usize, p: usize) -> LuaResult<Option<usize>> {
        if p + 1 >= self.pat.len() {
            return Err(pattern_error(
                "malformed pattern (missing arguments to '%b')",
            ));
        }
        let (open, close) = (self.pat[p], self.pat[p + 1]);
        if self.src.get(s) != Some(&open) {
            return Ok(None);
        }
        let mut depth = 1_usize;
        for (offset, &byte) in self.src[s + 1..].iter().enumerate() {
            self.meter.step()?;
            if byte == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(s + offset + 2));
                }
            } else if byte == open {
                depth += 1;
            }
        }
        Ok(None)
    }

    /// `%f[set]` with `p` at the expected `[`; returns the pattern offset
    /// after the set when the frontier matches at `s`.
    fn match_frontier(&mut self, s: usize, p: usize) -> LuaResult<Option<usize>> {
        if self.pat.get(p) != Some(&b'[') {
            return Err(pattern_error("missing '[' after '%f' in pattern"));
        }
        let ep = class_end(self.pat, p)?;
        let previous = if s == 0 { 0 } else { self.src[s - 1] };
        let current = self.src.get(s).copied().unwrap_or(0);
        let frontier = !matches_set(self.pat, previous, p, ep - 1)
            && matches_set(self.pat, current, p, ep - 1);
        Ok(frontier.then_some(ep))
    }

    fn start_capture(&mut self, s: usize, p: usize, len: CaptureLen) -> LuaResult<Option<usize>> {
        if self.level >= MAX_CAPTURES {
            return Err(pattern_error("too many captures"));
        }
        self.captures[self.level] = Capture { start: s, len };
        self.level += 1;
        let outcome = self.do_match(s, p)?;
        if outcome.is_none() {
            self.level -= 1;
        }
        Ok(outcome)
    }

    fn end_capture(&mut self, s: usize, p: usize) -> LuaResult<Option<usize>> {
        let index = (0..self.level)
            .rev()
            .find(|&index| self.captures[index].len == CaptureLen::Unfinished)
            .ok_or_else(|| pattern_error("invalid pattern capture"))?;
        self.captures[index].len = CaptureLen::Closed(s - self.captures[index].start);
        let outcome = self.do_match(s, p)?;
        if outcome.is_none() {
            self.captures[index].len = CaptureLen::Unfinished;
        }
        Ok(outcome)
    }

    /// A back-reference `%1`..`%9` at subject offset `s`.
    fn match_capture(&mut self, s: usize, digit: u8) -> LuaResult<Option<usize>> {
        let index = usize::from(digit).wrapping_sub(usize::from(b'1'));
        let capture = self
            .captures
            .get(index)
            .filter(|capture| index < self.level && capture.len != CaptureLen::Unfinished)
            .ok_or_else(|| {
                pattern_error(format!(
                    "invalid capture index %{}",
                    i32::from(digit) - i32::from(b'0')
                ))
            })?;
        let CaptureLen::Closed(len) = capture.len else {
            return Ok(None);
        };
        self.meter.charge(1 + len / 16)?;
        let matches = self.src.len() - s >= len
            && self.src[capture.start..capture.start + len] == self.src[s..s + len];
        Ok(matches.then_some(s + len))
    }
}
