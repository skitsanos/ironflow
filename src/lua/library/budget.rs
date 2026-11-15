//! Work budget for the Rust-implemented string-library calls (IF-213).
//!
//! The instruction hook cannot run inside a library call, so the calls that
//! can do unbounded work count their own steps and consult this budget.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use mlua::prelude::*;

use crate::util::execution::ExecutionControl;

/// Steps between two budget, deadline and cancellation checks.
const CHECK_INTERVAL: u32 = 4096;

/// Why a string-library call was stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LibraryLimitError {
    /// The Lua state spent its pattern-matching step budget.
    PatternSteps { max: u64 },
    /// The Lua state's wall-clock budget elapsed inside the call.
    Seconds { max: u64 },
    /// The step was cancelled or its deadline passed inside the call.
    Stopped(String),
    /// The result outgrew the Lua memory cap.
    Memory { max: u64 },
}

impl std::fmt::Display for LibraryLimitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PatternSteps { max } => write!(
                formatter,
                "Lua pattern matching exceeded step budget of {max} \
                 (IRONFLOW_LUA_MAX_PATTERN_STEPS)"
            ),
            Self::Seconds { max } => {
                write!(formatter, "Lua execution exceeded time budget of {max}s")
            }
            Self::Stopped(reason) => formatter.write_str(reason),
            Self::Memory { max } => write!(
                formatter,
                "Lua string result exceeded memory limit of {max} bytes \
                 (IRONFLOW_LUA_MAX_MEMORY_BYTES)"
            ),
        }
    }
}

impl std::error::Error for LibraryLimitError {}

impl From<LibraryLimitError> for LuaError {
    fn from(error: LibraryLimitError) -> Self {
        LuaError::external(error)
    }
}

/// Budgets of one Lua state that apply inside library calls.
#[derive(Debug)]
pub(crate) struct LibraryBudget {
    execution: Option<ExecutionControl>,
    started: Instant,
    max_seconds: Option<u64>,
    max_pattern_steps: Option<u64>,
    remaining_pattern_steps: AtomicI64,
    max_result_bytes: Option<u64>,
}

impl LibraryBudget {
    pub(crate) fn new(
        execution: Option<ExecutionControl>,
        max_seconds: Option<u64>,
        max_pattern_steps: Option<u64>,
        max_result_bytes: Option<u64>,
    ) -> Self {
        Self {
            execution,
            started: Instant::now(),
            max_seconds,
            max_pattern_steps,
            remaining_pattern_steps: AtomicI64::new(
                max_pattern_steps.map_or(i64::MAX, |max| max.min(i64::MAX as u64) as i64),
            ),
            max_result_bytes,
        }
    }

    /// The budget installed for `lua`, if its limits were applied.
    pub(crate) fn of(lua: &Lua) -> Option<Arc<Self>> {
        lua.app_data_ref::<Arc<Self>>().map(|budget| budget.clone())
    }

    fn spend(&self, steps: u32) -> Result<(), LibraryLimitError> {
        if let Some(max) = self.max_pattern_steps
            && self
                .remaining_pattern_steps
                .fetch_sub(i64::from(steps), Ordering::Relaxed)
                < i64::from(steps)
        {
            return Err(LibraryLimitError::PatternSteps { max });
        }
        if let Some(execution) = &self.execution {
            execution
                .checkpoint()
                .map_err(|error| LibraryLimitError::Stopped(error.to_string()))?;
        }
        match self.max_seconds {
            Some(max) if self.started.elapsed() >= Duration::from_secs(max) => {
                Err(LibraryLimitError::Seconds { max })
            }
            _ => Ok(()),
        }
    }

    /// Fail when a result built outside the Lua allocator outgrows the cap
    /// the allocator would have enforced.
    pub(crate) fn check_result_bytes(&self, bytes: usize) -> Result<(), LibraryLimitError> {
        match self.max_result_bytes {
            Some(max) if bytes as u64 > max => Err(LibraryLimitError::Memory { max }),
            _ => Ok(()),
        }
    }
}

/// Step counter of one library call.
pub(crate) struct Meter {
    budget: Option<Arc<LibraryBudget>>,
    pending: u32,
}

impl Meter {
    pub(crate) fn new(budget: Option<Arc<LibraryBudget>>) -> Self {
        Self { budget, pending: 0 }
    }

    pub(crate) fn budget(&self) -> Option<&LibraryBudget> {
        self.budget.as_deref()
    }

    /// Count one unit of matching work.
    #[inline]
    pub(crate) fn step(&mut self) -> Result<(), LibraryLimitError> {
        self.charge(1)
    }

    /// Count `steps` units of work, checking the budgets every
    /// [`CHECK_INTERVAL`] units.
    #[inline]
    pub(crate) fn charge(&mut self, steps: usize) -> Result<(), LibraryLimitError> {
        self.pending = self
            .pending
            .saturating_add(u32::try_from(steps).unwrap_or(u32::MAX));
        if self.pending >= CHECK_INTERVAL {
            self.settle()
        } else {
            Ok(())
        }
    }

    /// Account for the steps counted since the last check.
    pub(crate) fn settle(&mut self) -> Result<(), LibraryLimitError> {
        let steps = std::mem::take(&mut self.pending);
        match &self.budget {
            Some(budget) if steps > 0 => budget.spend(steps),
            _ => Ok(()),
        }
    }
}
