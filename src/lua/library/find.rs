//! `string.find`, `string.match` and `string.gmatch` over the budgeted
//! matcher (IF-213).

use std::sync::atomic::{AtomicUsize, Ordering};

use mlua::prelude::*;

use super::args::{Args, relay};
use super::budget::{LibraryBudget, Meter};
use super::pattern::{Captured, Matcher};

/// Pattern characters that rule out a plain substring search.
const SPECIALS: &[u8] = b"^$*+?.([%-";

/// A one-based, possibly negative start position as a zero-based offset
/// (`posrelatI` minus one).
fn start_offset(position: Option<LuaInteger>, len: usize) -> usize {
    let len = len as i64;
    match position.unwrap_or(1) {
        position if position > 0 => (position - 1) as usize,
        0 => 0,
        position if position < -len => 0,
        position => (len + position) as usize,
    }
}

pub(super) fn capture_value(lua: &Lua, src: &[u8], captured: Captured) -> LuaResult<LuaValue> {
    Ok(match captured {
        Captured::Text(start, end) => LuaValue::String(lua.create_string(&src[start..end])?),
        Captured::Position(position) => LuaValue::Integer(position as LuaInteger),
    })
}

fn capture_values(
    lua: &Lua,
    src: &[u8],
    matcher: &Matcher<'_>,
    start: usize,
    end: usize,
    whole: bool,
) -> LuaResult<Vec<LuaValue>> {
    matcher
        .captured(start, end, whole)?
        .into_iter()
        .map(|captured| capture_value(lua, src, captured))
        .collect()
}

/// First occurrence of `needle` in `haystack`, counting the comparison work.
fn plain_find(haystack: &[u8], needle: &[u8], meter: &mut Meter) -> LuaResult<Option<usize>> {
    let Some(&first) = needle.first() else {
        return Ok(Some(0));
    };
    let mut from = 0;
    while haystack.len() - from >= needle.len() {
        let last_start = haystack.len() - needle.len();
        let Some(skipped) = haystack[from..=last_start]
            .iter()
            .position(|&byte| byte == first)
        else {
            meter.charge(1 + (last_start - from) / 64)?;
            return Ok(None);
        };
        let candidate = from + skipped;
        meter.charge(1 + skipped / 64 + needle.len() / 16)?;
        if haystack[candidate..candidate + needle.len()] == *needle {
            return Ok(Some(candidate));
        }
        from = candidate + 1;
    }
    Ok(None)
}

fn find_or_match(lua: &Lua, args: &Args, find: bool) -> LuaResult<LuaMultiValue> {
    let (subject, pattern) = (args.string(lua, 0)?, args.string(lua, 1)?);
    let init = args.optional_integer(lua, 2)?;
    let plain = if find { args.value(3) } else { LuaValue::Nil };
    let (src, pat) = (subject.as_bytes(), pattern.as_bytes());
    let (src, pat): (&[u8], &[u8]) = (&src, &pat);
    let fail = || Ok(LuaMultiValue::from_iter([LuaValue::Nil]));
    let init = start_offset(init, src.len());
    if init > src.len() {
        return fail();
    }

    let mut meter = Meter::new(LibraryBudget::of(lua));
    let plain = !matches!(plain, LuaValue::Nil | LuaValue::Boolean(false));
    if find && (plain || !pat.iter().any(|byte| SPECIALS.contains(byte))) {
        let found = plain_find(&src[init..], pat, &mut meter)?;
        meter.settle()?;
        return match found {
            Some(offset) => {
                let start = init + offset;
                Ok(LuaMultiValue::from_iter([
                    LuaValue::Integer(start as LuaInteger + 1),
                    LuaValue::Integer((start + pat.len()) as LuaInteger),
                ]))
            }
            None => fail(),
        };
    }

    let anchored = pat.first() == Some(&b'^');
    let mut matcher = Matcher::new(src, pat, meter);
    let mut start = init;
    loop {
        if let Some(end) = matcher.match_at(start, usize::from(anchored))? {
            matcher.meter.settle()?;
            let mut values = Vec::new();
            if find {
                values.push(LuaValue::Integer(start as LuaInteger + 1));
                values.push(LuaValue::Integer(end as LuaInteger));
            }
            values.extend(capture_values(lua, src, &matcher, start, end, !find)?);
            return Ok(LuaMultiValue::from_iter(values));
        }
        if anchored || start >= src.len() {
            matcher.meter.settle()?;
            return fail();
        }
        start += 1;
    }
}

pub(super) fn find(lua: &Lua, args: LuaMultiValue) -> LuaResult<LuaMultiValue> {
    relay(lua, find_or_match(lua, &Args::new("find", args), true))
}

pub(super) fn string_match(lua: &Lua, args: LuaMultiValue) -> LuaResult<LuaMultiValue> {
    relay(lua, find_or_match(lua, &Args::new("match", args), false))
}

/// No match has ended yet.
const NO_MATCH: usize = usize::MAX;

pub(super) fn gmatch(lua: &Lua, args: LuaMultiValue) -> LuaResult<LuaMultiValue> {
    let iterator = iterator(lua, &Args::new("gmatch", args))
        .map(|function| LuaMultiValue::from_iter([LuaValue::Function(function)]));
    relay(lua, iterator)
}

fn iterator(lua: &Lua, args: &Args) -> LuaResult<LuaFunction> {
    let (subject, pattern) = (args.string(lua, 0)?, args.string(lua, 1)?);
    let init = args.optional_integer(lua, 2)?;
    let len = subject.as_bytes().len();
    // A start past the end yields no match, not even an empty one.
    let next = AtomicUsize::new(start_offset(init, len).min(len + 1));
    let last_end = AtomicUsize::new(NO_MATCH);
    lua.create_function(move |lua, ()| {
        relay(lua, next_match(lua, &subject, &pattern, &next, &last_end))
    })
}

fn next_match(
    lua: &Lua,
    subject: &LuaString,
    pattern: &LuaString,
    next: &AtomicUsize,
    last_end: &AtomicUsize,
) -> LuaResult<LuaMultiValue> {
    {
        let (src, pat) = (subject.as_bytes(), pattern.as_bytes());
        let (src, pat): (&[u8], &[u8]) = (&src, &pat);
        let mut matcher = Matcher::new(src, pat, Meter::new(LibraryBudget::of(lua)));
        for start in next.load(Ordering::Relaxed)..=src.len() {
            let Some(end) = matcher.match_at(start, 0)? else {
                continue;
            };
            if end == last_end.load(Ordering::Relaxed) {
                continue;
            }
            matcher.meter.settle()?;
            next.store(end, Ordering::Relaxed);
            last_end.store(end, Ordering::Relaxed);
            let values = capture_values(lua, src, &matcher, start, end, true)?;
            return Ok(LuaMultiValue::from_iter(values));
        }
        matcher.meter.settle()?;
        next.store(src.len() + 1, Ordering::Relaxed);
        Ok(LuaMultiValue::new())
    }
}
