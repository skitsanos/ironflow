//! `string.gsub` over the budgeted matcher (IF-213).

use mlua::prelude::*;

use super::args::{Args, callback_error, relay};
use super::budget::{LibraryBudget, Meter};
use super::classes::ESC;
use super::find::capture_value;
use super::pattern::{Captured, Matcher, pattern_error};

/// What replaces each match.
enum Replacement {
    Template(LuaString),
    Table(LuaTable),
    Function(LuaFunction),
}

impl Replacement {
    fn from_args(lua: &Lua, args: &Args) -> LuaResult<Self> {
        match args.value(2) {
            LuaValue::Table(table) => Ok(Self::Table(table)),
            LuaValue::Function(function) => Ok(Self::Function(function)),
            LuaValue::String(_) | LuaValue::Integer(_) | LuaValue::Number(_) => {
                args.string(lua, 2).map(Self::Template)
            }
            _ => Err(args.expected(2, "string/function/table")),
        }
    }
}

/// Appends replacements to the result and enforces its size cap.
struct Output<'a> {
    lua: &'a Lua,
    src: &'a [u8],
    bytes: Vec<u8>,
}

impl Output<'_> {
    fn push(&mut self, bytes: &[u8], matcher: &Matcher<'_>) -> LuaResult<()> {
        self.bytes.extend_from_slice(bytes);
        if let Some(budget) = matcher.meter.budget() {
            budget.check_result_bytes(self.bytes.len())?;
        }
        Ok(())
    }

    fn push_captured(&mut self, captured: Captured, matcher: &Matcher<'_>) -> LuaResult<()> {
        match captured {
            Captured::Text(start, end) => self.push(&self.src[start..end], matcher),
            Captured::Position(position) => self.push(position.to_string().as_bytes(), matcher),
        }
    }

    /// Expand `%0`..`%9` and `%%` of a template for the match `start..end`.
    fn push_template(
        &mut self,
        template: &[u8],
        matcher: &Matcher<'_>,
        start: usize,
        end: usize,
    ) -> LuaResult<()> {
        let mut rest = template;
        while let Some(escape) = rest.iter().position(|&byte| byte == ESC) {
            self.push(&rest[..escape], matcher)?;
            match rest.get(escape + 1) {
                Some(&ESC) => self.push(&[ESC], matcher)?,
                Some(b'0') => self.push(&self.src[start..end], matcher)?,
                Some(&digit) if digit.is_ascii_digit() => {
                    let index = usize::from(digit - b'1');
                    self.push_captured(matcher.capture(index, start, end)?, matcher)?;
                }
                _ => {
                    return Err(pattern_error("invalid use of '%' in replacement string"));
                }
            }
            rest = &rest[escape + 2..];
        }
        self.push(rest, matcher)
    }

    /// Append the replacement for the match `start..end`; returns whether it
    /// differs from keeping the match.
    fn push_replacement(
        &mut self,
        replacement: &Replacement,
        matcher: &Matcher<'_>,
        start: usize,
        end: usize,
    ) -> LuaResult<bool> {
        let value = match replacement {
            Replacement::Template(template) => {
                self.push_template(&template.as_bytes(), matcher, start, end)?;
                return Ok(true);
            }
            Replacement::Table(table) => {
                let key = capture_value(self.lua, self.src, matcher.capture(0, start, end)?)?;
                table.get::<LuaValue>(key).map_err(callback_error)?
            }
            Replacement::Function(function) => {
                let arguments = matcher
                    .captured(start, end, true)?
                    .into_iter()
                    .map(|captured| capture_value(self.lua, self.src, captured))
                    .collect::<LuaResult<Vec<_>>>()?;
                function
                    .call::<LuaValue>(LuaMultiValue::from_iter(arguments))
                    .map_err(callback_error)?
            }
        };
        match value {
            LuaValue::Nil | LuaValue::Boolean(false) => {
                self.push(&self.src[start..end], matcher)?;
                Ok(false)
            }
            LuaValue::String(_) | LuaValue::Integer(_) | LuaValue::Number(_) => {
                let text = self
                    .lua
                    .coerce_string(value)?
                    .ok_or_else(|| pattern_error("invalid replacement value (a number)"))?;
                self.push(&text.as_bytes(), matcher)?;
                Ok(true)
            }
            other => Err(pattern_error(format!(
                "invalid replacement value (a {})",
                other.type_name()
            ))),
        }
    }
}

pub(super) fn gsub(lua: &Lua, args: LuaMultiValue) -> LuaResult<LuaMultiValue> {
    let outcome = substitute(lua, &Args::new("gsub", args)).map(|(text, count)| {
        LuaMultiValue::from_iter([LuaValue::String(text), LuaValue::Integer(count)])
    });
    relay(lua, outcome)
}

fn substitute(lua: &Lua, args: &Args) -> LuaResult<(LuaString, LuaInteger)> {
    let (subject, pattern) = (args.string(lua, 0)?, args.string(lua, 1)?);
    let replacement = Replacement::from_args(lua, args)?;
    let max = args.optional_integer(lua, 3)?;
    let (src_bytes, pat_bytes) = (subject.as_bytes(), pattern.as_bytes());
    let (src, pat): (&[u8], &[u8]) = (&src_bytes, &pat_bytes);
    let anchored = pat.first() == Some(&b'^');
    let max = max.unwrap_or(src.len() as LuaInteger + 1);

    let mut matcher = Matcher::new(src, pat, Meter::new(LibraryBudget::of(lua)));
    let mut output = Output {
        lua,
        src,
        bytes: Vec::new(),
    };
    let (mut position, mut last_end, mut count, mut changed) = (0, None, 0, false);
    while count < max {
        match matcher.match_at(position, usize::from(anchored))? {
            Some(end) if Some(end) != last_end => {
                count += 1;
                changed |= output.push_replacement(&replacement, &matcher, position, end)?;
                position = end;
                last_end = Some(end);
            }
            _ if position < src.len() => {
                output.push(&src[position..=position], &matcher)?;
                position += 1;
            }
            _ => break,
        }
        if anchored {
            break;
        }
    }
    matcher.meter.settle()?;
    if !changed {
        return Ok((subject, count));
    }
    output.push(&src[position..], &matcher)?;
    Ok((lua.create_string(&output.bytes)?, count))
}
