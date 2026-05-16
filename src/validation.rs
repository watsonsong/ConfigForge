use std::collections::HashSet;

use crate::{
    meta::{LinkedFieldMeta, LinkedTableMeta, PrimitiveFieldType},
    source::DataTable,
    ConfigForgeError, Result,
};

pub fn validate_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    for field in &meta.fields {
        validate_field_presence(meta, table, field)?;
        validate_field_values(meta, table, field)?;
        validate_field_uniqueness(meta, table, field)?;
    }

    for index in &meta.indexes {
        validate_index_uniqueness(meta, table, index)?;
    }

    Ok(())
}

fn validate_index_uniqueness(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    index: &crate::meta::LinkedIndexMeta<'_>,
) -> Result<()> {
    if !index.meta.unique {
        return Ok(());
    }

    let mut seen = HashSet::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let key = index
            .fields
            .iter()
            .map(|field| row.get(&field.name).cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        if !seen.insert(key.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} index `{}` duplicates unique key `{:?}`",
                meta.meta.name,
                row_index + 1,
                index.meta.name,
                key
            )));
        }
    }

    Ok(())
}

fn validate_field_presence(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    let present = table
        .rows
        .iter()
        .all(|row| row.contains_key(&field.meta.name));
    if !present {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` is missing field `{}`",
            meta.meta.name, field.meta.name
        )));
    }
    Ok(())
}

fn validate_field_values(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    for (row_index, row) in table.rows.iter().enumerate() {
        let value = row
            .get(&field.meta.name)
            .map(String::as_str)
            .unwrap_or_default();
        if field.meta.required && value.trim().is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` is required",
                meta.meta.name,
                row_index + 1,
                field.meta.name
            )));
        }

        if value.trim().is_empty() {
            continue;
        }

        let valid = match field.primitive_type() {
            Some(PrimitiveFieldType::Int) => value.parse::<i64>().is_ok(),
            Some(PrimitiveFieldType::Float) => value.parse::<f64>().is_ok(),
            Some(PrimitiveFieldType::Bool) => matches!(value, "true" | "false" | "1" | "0"),
            Some(
                PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
            )
            | None => true,
        };

        if !valid {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
                meta.meta.name,
                row_index + 1,
                field.meta.name,
                value,
                field.meta.field_type
            )));
        }
    }
    Ok(())
}

fn validate_field_uniqueness(
    meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if !field.meta.unique {
        return Ok(());
    }

    let mut seen = HashSet::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let value = row
            .get(&field.meta.name)
            .map(String::as_str)
            .unwrap_or_default();
        if !seen.insert(value.to_owned()) {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` duplicates unique value `{}`",
                meta.meta.name,
                row_index + 1,
                field.meta.name,
                value
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::meta::{
        FieldMeta, LinkedField, LinkedFieldMeta, LinkedTableMeta, PrimitiveFieldType, TableMeta,
    };

    use super::*;

    #[test]
    fn rejects_invalid_type() {
        let meta = TableMeta {
            name: "items".to_owned(),
            kind: crate::meta::TableKind::Collection,
            desc: None,
            source: Some("items.csv".to_owned()),
            sources: vec![],
            override_sources: vec![],
            fields: vec![FieldMeta {
                name: "id".to_owned(),
                desc: None,
                field_type: "int".to_owned(),
                required: true,
                default: None,
                layout: None,
                export: true,
                tool_only: false,
                filter_field: false,
                validator_only: false,
                field_number: None,
                unique: true,
            }],
            indexes: vec![],
            primary_index: None,
            preserve_source_order: false,
            polymorphic: false,
        };
        let linked_meta = LinkedTableMeta::new(
            &meta,
            vec![LinkedFieldMeta {
                meta: &meta.fields[0],
                linked_field: LinkedField::Primitive(PrimitiveFieldType::Int),
            }],
            vec![],
        );
        let table = DataTable {
            name: "items".to_owned(),
            rows: vec![BTreeMap::from([("id".to_owned(), "abc".to_owned())])],
        };

        assert!(validate_table(&linked_meta, &table).is_err());
    }
}
