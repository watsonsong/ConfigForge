use std::collections::HashSet;

use crate::{
    meta::{FieldMeta, FieldType, TableMeta},
    source::DataTable,
    ConfigForgeError, Result,
};

pub fn validate_table(meta: &TableMeta, table: &DataTable) -> Result<()> {
    for field in &meta.fields {
        validate_field_presence(meta, table, field)?;
        validate_field_values(meta, table, field)?;
        validate_field_uniqueness(meta, table, field)?;
    }

    for index in &meta.indexes {
        for field_name in &index.fields {
            if !meta.fields.iter().any(|field| &field.name == field_name) {
                return Err(ConfigForgeError::Validation(format!(
                    "table `{}` index `{}` references unknown field `{}`",
                    meta.name, index.name, field_name
                )));
            }
        }
    }

    Ok(())
}

fn validate_field_presence(meta: &TableMeta, table: &DataTable, field: &FieldMeta) -> Result<()> {
    let present = table.rows.iter().all(|row| row.contains_key(&field.name));
    if !present {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` is missing field `{}`",
            meta.name, field.name
        )));
    }
    Ok(())
}

fn validate_field_values(meta: &TableMeta, table: &DataTable, field: &FieldMeta) -> Result<()> {
    for (row_index, row) in table.rows.iter().enumerate() {
        let value = row.get(&field.name).map(String::as_str).unwrap_or_default();
        if field.required && value.trim().is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` is required",
                meta.name,
                row_index + 1,
                field.name
            )));
        }

        if value.trim().is_empty() {
            continue;
        }

        let valid = match field.field_type {
            FieldType::String => true,
            FieldType::I64 => value.parse::<i64>().is_ok(),
            FieldType::F64 => value.parse::<f64>().is_ok(),
            FieldType::Bool => matches!(value, "true" | "false" | "1" | "0"),
        };

        if !valid {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
                meta.name,
                row_index + 1,
                field.name,
                value,
                field.field_type
            )));
        }
    }
    Ok(())
}

fn validate_field_uniqueness(meta: &TableMeta, table: &DataTable, field: &FieldMeta) -> Result<()> {
    if !field.unique {
        return Ok(());
    }

    let mut seen = HashSet::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let value = row.get(&field.name).map(String::as_str).unwrap_or_default();
        if !seen.insert(value.to_owned()) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` duplicates unique value `{}`",
                meta.name,
                row_index + 1,
                field.name,
                value
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::meta::{FieldMeta, FieldType, TableMeta};

    use super::*;

    #[test]
    fn rejects_invalid_type() {
        let meta = TableMeta {
            name: "items".to_owned(),
            source: "items.csv".to_owned(),
            fields: vec![FieldMeta {
                name: "id".to_owned(),
                field_type: FieldType::I64,
                required: true,
                unique: true,
            }],
            indexes: vec![],
        };
        let table = DataTable {
            name: "items".to_owned(),
            rows: vec![BTreeMap::from([("id".to_owned(), "abc".to_owned())])],
        };

        assert!(validate_table(&meta, &table).is_err());
    }
}
