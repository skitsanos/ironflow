//! Argument checks with the reference library's messages, and the protocol
//! that turns a Rust failure into an ordinary Lua string error.

use std::ffi::c_void;

use mlua::prelude::*;

use super::budget::LibraryLimitError;

static FAIL_MARKER: u8 = 0;
static RAW_MARKER: u8 = 0;

fn marker(address: &'static u8) -> LuaValue {
    LuaValue::LightUserData(LuaLightUserData(
        std::ptr::from_ref(address).cast_mut().cast::<c_void>(),
    ))
}

/// First result of a call that failed in the library itself; the Lua side
/// raises the second result at the caller's position.
pub(super) fn fail_marker() -> LuaValue {
    marker(&FAIL_MARKER)
}

/// First result of a call whose Lua callback failed; the Lua side raises the
/// second result unchanged.
pub(super) fn raw_marker() -> LuaValue {
    marker(&RAW_MARKER)
}

/// A string error raised by a Lua callback of `string.gsub`.
#[derive(Debug)]
struct CallbackFailure(String);

impl std::fmt::Display for CallbackFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CallbackFailure {}

/// Keep a callback's string error apart from the library's own errors. The
/// traceback mlua appends to the message is dropped, so the caller sees the
/// value the callback raised.
pub(super) fn callback_error(error: LuaError) -> LuaError {
    match error {
        LuaError::RuntimeError(mut message) => {
            if let Some(traceback) = message.rfind("\nstack traceback:\n") {
                message.truncate(traceback);
            }
            LuaError::external(CallbackFailure(message))
        }
        other => other,
    }
}

/// Report a library or callback failure as `(marker, message)` results; any
/// other error (a budget error raised by the hook, for example) stays a Lua
/// error object.
pub(super) fn relay(lua: &Lua, outcome: LuaResult<LuaMultiValue>) -> LuaResult<LuaMultiValue> {
    let error = match outcome {
        Ok(values) => return Ok(values),
        Err(error) => error,
    };
    let (marker, message) = match &error {
        LuaError::RuntimeError(message) => (fail_marker(), message.clone()),
        LuaError::ExternalError(external) => {
            if let Some(limit) = external.downcast_ref::<LibraryLimitError>() {
                (fail_marker(), limit.to_string())
            } else if let Some(CallbackFailure(message)) = external.downcast_ref() {
                (raw_marker(), message.clone())
            } else {
                return Err(error);
            }
        }
        _ => return Err(error),
    };
    Ok(LuaMultiValue::from_iter([
        marker,
        LuaValue::String(lua.create_string(message)?),
    ]))
}

fn type_name(value: Option<&LuaValue>) -> &'static str {
    match value {
        None => "no value",
        Some(LuaValue::Integer(_) | LuaValue::Number(_)) => "number",
        Some(LuaValue::LightUserData(_)) => "light userdata",
        Some(value) => value.type_name(),
    }
}

/// The arguments of one call to the library function `name`.
pub(super) struct Args {
    name: &'static str,
    values: LuaMultiValue,
}

impl Args {
    pub(super) fn new(name: &'static str, values: LuaMultiValue) -> Self {
        Self { name, values }
    }

    pub(super) fn bad(&self, index: usize, problem: impl std::fmt::Display) -> LuaError {
        LuaError::runtime(format!(
            "bad argument #{} to '{}' ({problem})",
            index + 1,
            self.name
        ))
    }

    pub(super) fn expected(&self, index: usize, expected: &str) -> LuaError {
        let found = type_name(self.values.get(index));
        self.bad(index, format_args!("{expected} expected, got {found}"))
    }

    pub(super) fn value(&self, index: usize) -> LuaValue {
        self.values.get(index).cloned().unwrap_or(LuaValue::Nil)
    }

    /// A string argument; numbers convert as in the reference library.
    pub(super) fn string(&self, lua: &Lua, index: usize) -> LuaResult<LuaString> {
        match self.values.get(index) {
            Some(value @ (LuaValue::String(_) | LuaValue::Integer(_) | LuaValue::Number(_))) => lua
                .coerce_string(value.clone())?
                .ok_or_else(|| self.expected(index, "string")),
            _ => Err(self.expected(index, "string")),
        }
    }

    /// An optional integer argument.
    pub(super) fn optional_integer(
        &self,
        lua: &Lua,
        index: usize,
    ) -> LuaResult<Option<LuaInteger>> {
        let value = match self.values.get(index) {
            None | Some(LuaValue::Nil) => return Ok(None),
            Some(value) => value,
        };
        if let Some(integer) = lua.coerce_integer(value.clone())? {
            return Ok(Some(integer));
        }
        if lua.coerce_number(value.clone())?.is_some() {
            return Err(self.bad(index, "number has no integer representation"));
        }
        Err(self.expected(index, "number"))
    }
}
