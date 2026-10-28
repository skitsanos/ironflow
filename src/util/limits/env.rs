//! One parsing contract for the numeric `IRONFLOW_*` limits.
//!
//! Every limit is declared once as a [`Limit`] (or [`Toggle`]) and listed in
//! `limits::ALL` / `limits::TOGGLES`, so startup can validate the whole
//! environment before a run, a server, or a validation does any work. Node
//! code reads limits lazily through [`Limit::read`]; after startup validation
//! an invalid value can only appear if the environment changed afterwards,
//! which is logged and ignored instead of silently accepted.

use anyhow::{Result, bail};

/// How a limit treats the value `0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zero {
    /// A hard ceiling: `0` is rejected like any other invalid value.
    Rejected,
    /// `0` disables the limit, as the variable's documentation states.
    Disables,
}

/// A numeric environment limit with its default and zero semantics.
#[derive(Debug)]
pub struct Limit {
    pub name: &'static str,
    pub default: u64,
    pub zero: Zero,
}

impl Limit {
    pub const fn positive(name: &'static str, default: u64) -> Self {
        Self {
            name,
            default,
            zero: Zero::Rejected,
        }
    }

    pub const fn optional(name: &'static str, default: u64) -> Self {
        Self {
            name,
            default,
            zero: Zero::Disables,
        }
    }

    fn expected(&self) -> &'static str {
        match self.zero {
            Zero::Rejected => "a positive integer",
            Zero::Disables => "a non-negative integer (0 disables the limit)",
        }
    }

    /// Parse one raw value under this limit's contract: a base-10 integer
    /// without sign or unit, surrounded by optional whitespace.
    pub fn parse(&self, raw: &str) -> Result<u64> {
        let digits = raw.trim();
        let value = digits
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| digits.parse::<u64>().ok())
            .flatten()
            .filter(|value| *value > 0 || self.zero == Zero::Disables);
        match value {
            Some(value) => Ok(value),
            None => bail!("{} must be {}", self.name, self.expected()),
        }
    }

    /// Resolve the configured value: the default when unset, otherwise the
    /// parsed override. Errors name the variable and the accepted format.
    pub fn resolve(&self) -> Result<u64> {
        match std::env::var(self.name) {
            Ok(raw) => self.parse(&raw),
            Err(std::env::VarError::NotPresent) => Ok(self.default),
            Err(std::env::VarError::NotUnicode(_)) => {
                bail!("{} must contain valid UTF-8", self.name)
            }
        }
    }

    /// Read the value for node use. Startup validation has already rejected an
    /// invalid override; one that appears later is logged and the default
    /// applies, so a ceiling never silently moves.
    pub fn read(&self) -> u64 {
        self.resolve().unwrap_or_else(|error| {
            tracing::warn!(
                limit = self.name,
                %error,
                "ignoring an invalid limit override; using the default"
            );
            self.default
        })
    }

    /// [`Limit::read`] for limits whose documented `0` disables them.
    pub fn read_optional(&self) -> Option<u64> {
        let value = self.read();
        (value > 0).then_some(value)
    }
}

/// A boolean environment switch that accepts only `true` or `false`.
#[derive(Debug)]
pub struct Toggle {
    pub name: &'static str,
    pub default: bool,
}

impl Toggle {
    pub const fn new(name: &'static str, default: bool) -> Self {
        Self { name, default }
    }

    pub fn parse(&self, raw: &str) -> Result<bool> {
        match raw.trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => bail!("{} must be either 'true' or 'false'", self.name),
        }
    }

    pub fn resolve(&self) -> Result<bool> {
        match std::env::var(self.name) {
            Ok(raw) => self.parse(&raw),
            Err(std::env::VarError::NotPresent) => Ok(self.default),
            Err(std::env::VarError::NotUnicode(_)) => {
                bail!("{} must contain valid UTF-8", self.name)
            }
        }
    }

    pub fn read(&self) -> bool {
        self.resolve().unwrap_or_else(|error| {
            tracing::warn!(
                toggle = self.name,
                %error,
                "ignoring an invalid toggle override; using the default"
            );
            self.default
        })
    }
}

/// Validate every registered limit once at startup. The first invalid
/// variable is reported by name together with the accepted format.
pub fn validate_environment() -> Result<()> {
    for limit in super::ALL {
        limit.resolve()?;
    }
    for toggle in super::TOGGLES {
        toggle.resolve()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CEILING: Limit = Limit::positive("IRONFLOW_TEST_CEILING", 7);
    const DISABLEABLE: Limit = Limit::optional("IRONFLOW_TEST_DISABLEABLE", 7);
    const SWITCH: Toggle = Toggle::new("IRONFLOW_TEST_SWITCH", true);

    #[test]
    fn ceilings_accept_only_positive_plain_integers() {
        assert_eq!(CEILING.parse(" 42 ").unwrap(), 42);
        for raw in ["0", "-1", "10MB", "1e6", "", "  ", "4.0", "+4"] {
            let error = CEILING.parse(raw).unwrap_err().to_string();
            assert_eq!(
                error, "IRONFLOW_TEST_CEILING must be a positive integer",
                "{raw:?}"
            );
        }
    }

    #[test]
    fn disableable_limits_accept_zero_and_reject_everything_else_invalid() {
        assert_eq!(DISABLEABLE.parse("0").unwrap(), 0);
        assert_eq!(DISABLEABLE.parse("12").unwrap(), 12);
        for raw in ["-1", "none", "1k", ""] {
            let error = DISABLEABLE.parse(raw).unwrap_err().to_string();
            assert_eq!(
                error,
                "IRONFLOW_TEST_DISABLEABLE must be a non-negative integer (0 disables the limit)",
                "{raw:?}"
            );
        }
    }

    #[test]
    fn toggles_accept_only_true_or_false() {
        assert!(SWITCH.parse("true").unwrap());
        assert!(!SWITCH.parse(" false ").unwrap());
        for raw in ["yes", "1", "TRUE", ""] {
            let error = SWITCH.parse(raw).unwrap_err().to_string();
            assert_eq!(
                error, "IRONFLOW_TEST_SWITCH must be either 'true' or 'false'",
                "{raw:?}"
            );
        }
    }

    /// Every `IRONFLOW_*` variable named in the limits module tree must be
    /// registered, or startup validation silently skips it.
    #[test]
    fn every_limit_variable_in_the_module_tree_is_registered() {
        let sources = [
            include_str!("../limits.rs"),
            include_str!("image.rs"),
            include_str!("pdf.rs"),
            include_str!("process.rs"),
            include_str!("xlsx.rs"),
        ];
        let registered: Vec<&str> = super::super::ALL
            .iter()
            .map(|limit| limit.name)
            .chain(super::super::TOGGLES.iter().map(|toggle| toggle.name))
            .collect();
        for source in sources {
            for name in source
                .split('"')
                .filter(|piece| piece.starts_with("IRONFLOW_"))
            {
                assert!(
                    registered.contains(&name),
                    "{name} is declared in the limits module tree but not registered"
                );
            }
        }
        let mut sorted = registered.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            registered.len(),
            "a limit is registered twice"
        );
    }
}
