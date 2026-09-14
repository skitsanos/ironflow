use anyhow::Result;
use serde_json::{Map, Number, Value, json};
use sqlx::any::{AnyRow, AnyTypeInfoKind};
use sqlx::{Any, Column, Decode, Row, Type, ValueRef};

pub(super) fn row_to_json(row: &AnyRow) -> Result<Value> {
    let mut map = Map::new();
    for column in row.columns() {
        let index = column.ordinal();
        let raw = row
            .try_get_raw(index)
            .map_err(|_| anyhow::anyhow!("db_query could not read column at index {index}"))?;
        // SQLite expressions can have NULL column metadata while their values
        // are numeric; a declared column can also change storage class per row.
        let value = match raw.type_info().kind() {
            AnyTypeInfoKind::Null => Value::Null,
            AnyTypeInfoKind::Bool => json!(decode::<bool>(row, index)?),
            AnyTypeInfoKind::SmallInt | AnyTypeInfoKind::Integer | AnyTypeInfoKind::BigInt => {
                json!(decode::<i64>(row, index)?)
            }
            AnyTypeInfoKind::Real => finite_number(f64::from(decode::<f32>(row, index)?), index)?,
            AnyTypeInfoKind::Double => finite_number(decode::<f64>(row, index)?, index)?,
            AnyTypeInfoKind::Text => Value::String(decode::<String>(row, index)?),
            AnyTypeInfoKind::Blob => json!(decode::<Vec<u8>>(row, index)?),
        };
        map.insert(column.name().to_owned(), value);
    }
    Ok(Value::Object(map))
}

fn decode<'r, T: Decode<'r, Any> + Type<Any>>(row: &'r AnyRow, index: usize) -> Result<T> {
    // Driver conversion errors can contain the cell value. Report the position
    // without retaining an error chain that could disclose database contents.
    row.try_get(index)
        .map_err(|_| anyhow::anyhow!("db_query could not decode column at index {index}"))
}

fn finite_number(value: f64, index: usize) -> Result<Value> {
    Number::from_f64(value).map(Value::Number).ok_or_else(|| {
        anyhow::anyhow!(
            "db_query cannot represent non-finite number at column index {index} in JSON"
        )
    })
}

#[cfg(test)]
mod tests;
