//! Comparison grammar for guard conditions: `<path> <operator> <literal>`.
//!
//! The right operand is always a literal. `!=` is the negation of `==` for
//! every operand pair, and a comparison that has no defined result is an
//! error rather than `false`, so a guard cannot skip its step silently.

use serde_json::Value;

/// Operators, two-character symbols first so `>=` is not read as `>`.
const OPERATORS: [(&str, Operator); 6] = [
    ("==", Operator::Equal),
    ("!=", Operator::NotEqual),
    (">=", Operator::GreaterOrEqual),
    ("<=", Operator::LessOrEqual),
    (">", Operator::Greater),
    ("<", Operator::Less),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Equal,
    NotEqual,
    GreaterOrEqual,
    LessOrEqual,
    Greater,
    Less,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Literal {
    Nil,
    Bool(bool),
    Number(f64),
    Text,
}

/// Why a comparison has no result. Messages name operands as written in the
/// flow and the JSON type of the context value, never the value itself.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ComparisonError {
    #[error("operator '{operator}' needs a number on the right, found '{found}'")]
    OrderingLiteral {
        operator: &'static str,
        found: String,
    },
    #[error("operator '{operator}' cannot order {path}, which holds {kind}")]
    OrderingValue {
        operator: &'static str,
        path: String,
        kind: &'static str,
    },
    #[error("{path} holds {kind}, which can only be compared with nil")]
    ContainerEquality { path: String, kind: &'static str },
    #[error("'{found}' is not a context path; and, or, not and grouping are not supported")]
    Path { found: String },
    #[error("'{found}' is a literal where a context path such as ctx.key is required")]
    LiteralPath { found: String },
    #[error(
        "right operand '{found}' must be one number, quoted string, true, false, nil or word; \
         and, or and not are not supported"
    )]
    Operand { found: String },
}

#[derive(Debug)]
pub(super) struct Comparison<'a> {
    pub(super) left: &'a str,
    symbol: &'static str,
    operator: Operator,
    right: &'a str,
    /// The operand's text without its quotes, compared with string values.
    text: &'a str,
    literal: Literal,
}

impl<'a> Comparison<'a> {
    /// Split a condition at its operator. `Ok(None)` means the condition is
    /// not a comparison. A comparison that is invalid whatever the context
    /// holds is rejected here, so flow loading can report it.
    pub(super) fn parse(condition: &'a str) -> Result<Option<Self>, ComparisonError> {
        let Some((position, symbol, operator)) = leftmost_operator(condition) else {
            return Ok(None);
        };
        let left = condition[..position].trim();
        context_path(left)?;
        let right = condition[position + symbol.len()..].trim();
        let (literal, text) = classify(right)?;
        let comparison = Self {
            left,
            symbol,
            operator,
            right,
            text,
            literal,
        };
        if comparison.is_ordering() && !matches!(comparison.literal, Literal::Number(_)) {
            return Err(comparison.ordering_literal_error());
        }
        Ok(Some(comparison))
    }

    pub(super) fn evaluate(&self, left: Option<&Value>) -> Result<bool, ComparisonError> {
        match self.operator {
            Operator::Equal => self.equals(left),
            Operator::NotEqual => self.equals(left).map(|equal| !equal),
            _ => self.orders(left),
        }
    }

    fn is_ordering(&self) -> bool {
        !matches!(self.operator, Operator::Equal | Operator::NotEqual)
    }

    fn equals(&self, left: Option<&Value>) -> Result<bool, ComparisonError> {
        Ok(match left {
            None | Some(Value::Null) => self.literal == Literal::Nil,
            Some(Value::Bool(value)) => self.literal == Literal::Bool(*value),
            Some(Value::Number(value)) => match (value.as_f64(), self.literal) {
                (Some(left), Literal::Number(right)) => (left - right).abs() < f64::EPSILON,
                _ => false,
            },
            // A string is compared with the operand's text, quoted or not.
            Some(Value::String(value)) => value == self.text,
            Some(_) if self.literal == Literal::Nil => false,
            Some(container) => {
                return Err(ComparisonError::ContainerEquality {
                    path: self.left.to_string(),
                    kind: kind_of(container),
                });
            }
        })
    }

    fn orders(&self, left: Option<&Value>) -> Result<bool, ComparisonError> {
        let Literal::Number(right) = self.literal else {
            return Err(self.ordering_literal_error());
        };
        match left {
            Some(Value::Number(value)) => {
                Ok(value.as_f64().is_some_and(|left| match self.operator {
                    Operator::Greater => left > right,
                    Operator::Less => left < right,
                    Operator::GreaterOrEqual => left >= right,
                    Operator::LessOrEqual => left <= right,
                    Operator::Equal | Operator::NotEqual => false,
                }))
            }
            // Nothing else is ordered: an absent value or a numeric string
            // must not skip the step as if the comparison had been false.
            other => Err(ComparisonError::OrderingValue {
                operator: self.symbol,
                path: self.left.to_string(),
                kind: other.map_or("nil", kind_of),
            }),
        }
    }

    fn ordering_literal_error(&self) -> ComparisonError {
        ComparisonError::OrderingLiteral {
            operator: self.symbol,
            found: self.right.to_string(),
        }
    }
}

/// The first operator in the condition. Everything after it is the right
/// operand, so an operator inside a string operand does not split it.
fn leftmost_operator(condition: &str) -> Option<(usize, &'static str, Operator)> {
    condition.char_indices().find_map(|(position, _)| {
        OPERATORS
            .iter()
            .find(|(symbol, _)| condition[position..].starts_with(symbol))
            .map(|(symbol, operator)| (position, *symbol, *operator))
    })
}

/// Classify the right operand: one quoted string, or one unquoted token.
fn classify(operand: &str) -> Result<(Literal, &str), ComparisonError> {
    let invalid = || ComparisonError::Operand {
        found: operand.to_string(),
    };
    if let Some(quote) = operand
        .chars()
        .next()
        .filter(|first| ['"', '\''].contains(first))
    {
        // A quote inside the string would hide a second clause in the text.
        return operand[1..]
            .strip_suffix(quote)
            .filter(|text| !text.contains(quote))
            .map(|text| (Literal::Text, text))
            .ok_or_else(invalid);
    }
    if operand.is_empty() || operand.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'')
    {
        return Err(invalid());
    }
    let literal = match operand {
        "nil" => Literal::Nil,
        "true" => Literal::Bool(true),
        "false" => Literal::Bool(false),
        _ => operand.parse().map_or(Literal::Text, Literal::Number),
    };
    Ok((literal, operand))
}

/// Check a left operand and return the path below `ctx.`. A path has no
/// whitespace, quotes, parentheses or operator characters, which is what
/// `not ctx.a`, `ctx.a and ctx.b` and `ctx.a ~= 1` would otherwise become.
pub(super) fn context_path(operand: &str) -> Result<&str, ComparisonError> {
    // `true` as a whole condition would otherwise be a lookup of a key named
    // "true". Such a key stays reachable with the prefix: `ctx.true`.
    let numeric = operand.starts_with(|c: char| c.is_ascii_digit() || "+-.".contains(c))
        && operand.parse::<f64>().is_ok();
    if numeric || ["true", "false", "nil"].contains(&operand) {
        return Err(ComparisonError::LiteralPath {
            found: operand.to_string(),
        });
    }
    let path = operand.strip_prefix("ctx.").unwrap_or(operand);
    let malformed = |c: char| c.is_whitespace() || "!=<>~()\"'&|".contains(c);
    if path.contains(malformed) || path.split('.').any(str::is_empty) {
        return Err(ComparisonError::Path {
            found: operand.to_string(),
        });
    }
    Ok(path)
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "nil",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
#[path = "comparison_tests.rs"]
mod tests;
