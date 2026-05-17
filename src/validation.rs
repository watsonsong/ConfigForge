use std::collections::HashSet;

use crate::{
    data::{DataSet, DataTable},
    meta::{LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedTableMeta, PrimitiveFieldType},
    ConfigForgeError, Result,
};

pub fn validate_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<()> {
    let data_set = normalize_data_set(meta, data_set)?;
    validate_normalized_data_set(meta, &data_set)
}

pub fn normalize_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<DataSet> {
    let mut tables = Vec::with_capacity(data_set.tables().len());
    for table in data_set.tables() {
        if let Some(table_meta) = meta.table(&table.name) {
            tables.push(normalize_table(table_meta, table)?);
        } else {
            tables.push(table.clone());
        }
    }
    DataSet::try_new(tables)
}

pub fn validate_normalized_data_set(meta: &LinkedMetaModel<'_>, data_set: &DataSet) -> Result<()> {
    for table_meta in &meta.tables {
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "data set is missing table `{}`",
                table_meta.meta.name
            ))
        })?;
        validate_normalized_table(table_meta, table)?;
    }

    Ok(())
}

pub fn validate_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    let table = normalize_table(meta, table)?;
    validate_normalized_table(meta, &table)
}

pub fn validate_normalized_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<()> {
    for field in &meta.fields {
        validate_supported_field_type(meta, field)?;
        validate_field_presence(meta, table, field)?;
        validate_field_values(meta, table, field)?;
        validate_field_uniqueness(meta, table, field)?;
    }

    for index in &meta.indexes {
        validate_index_uniqueness(meta, table, index)?;
    }

    Ok(())
}

pub fn normalize_table(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    apply_field_defaults(meta, table)
}

pub fn apply_field_defaults(meta: &LinkedTableMeta<'_>, table: &DataTable) -> Result<DataTable> {
    let mut table = table.clone();
    for row in &mut table.rows {
        for field in &meta.fields {
            let needs_default = row
                .get(&field.meta.name)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true);
            if !needs_default {
                continue;
            }

            if let Some(default) = &field.meta.default {
                row.insert(
                    field.meta.name.clone(),
                    default_value_to_cell(meta, field, default)?,
                );
            }
        }
    }

    Ok(table)
}

fn default_value_to_cell(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    default: &toml::Value,
) -> Result<String> {
    if let Some(value) = default.as_str() {
        return Ok(value.to_owned());
    }
    if let Some(value) = default.as_integer() {
        return Ok(value.to_string());
    }
    if let Some(value) = default.as_float() {
        return Ok(value.to_string());
    }
    if let Some(value) = default.as_bool() {
        return Ok(value.to_string());
    }

    Err(ConfigForgeError::Validation(format!(
        "table `{}` field `{}` has unsupported default value `{}`",
        meta.meta.name, field.meta.name, default
    )))
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
    if !field.meta.required {
        return Ok(());
    }

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

        validate_value_type(meta, field, value, row_index)?;
    }
    Ok(())
}

fn validate_supported_field_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
) -> Result<()> {
    if let LinkedField::Struct(_) = field.linked_field {
        return Err(ConfigForgeError::Validation(format!(
            "table `{}` field `{}` uses struct type `{}`, but struct value validation is not implemented yet",
            meta.meta.name, field.meta.name, field.meta.field_type
        )));
    }

    Ok(())
}

fn validate_value_type(
    meta: &LinkedTableMeta<'_>,
    field: &LinkedFieldMeta<'_>,
    value: &str,
    row_index: usize,
) -> Result<()> {
    let valid = match field.linked_field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => value.parse::<i64>().is_ok(),
        LinkedField::Primitive(PrimitiveFieldType::Float) => value.parse::<f64>().is_ok(),
        LinkedField::Primitive(PrimitiveFieldType::Bool) => {
            matches!(value, "true" | "false" | "1" | "0")
        }
        LinkedField::Primitive(
            PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
        ) => true,
        LinkedField::Enum(enum_meta) => enum_meta
            .values
            .iter()
            .any(|enum_value| enum_value.name == value),
        LinkedField::Struct(_) => {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` uses struct type `{}`, but struct value validation is not implemented yet",
                meta.meta.name,
                row_index + 1,
                field.meta.name,
                field.meta.field_type
            )));
        }
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
        EnumMeta, EnumValueMeta, FieldMeta, LinkedField, LinkedFieldMeta, LinkedTableMeta,
        MetaModel, PrimitiveFieldType, TableMeta,
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

    #[test]
    fn applies_defaults_before_required_and_type_validation() {
        let meta = TableMeta {
            name: "items".to_owned(),
            kind: crate::meta::TableKind::Collection,
            desc: None,
            source: Some("items.csv".to_owned()),
            sources: vec![],
            override_sources: vec![],
            fields: vec![FieldMeta {
                name: "max_stack".to_owned(),
                desc: None,
                field_type: "int".to_owned(),
                required: true,
                default: Some(toml::Value::Integer(1)),
                layout: None,
                export: true,
                tool_only: false,
                filter_field: false,
                validator_only: false,
                field_number: None,
                unique: false,
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
            rows: vec![BTreeMap::new()],
        };

        assert!(validate_table(&linked_meta, &table).is_ok());
        let table = apply_field_defaults(&linked_meta, &table).unwrap();
        assert_eq!(
            table.rows[0].get("max_stack").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn allows_missing_optional_field_without_default() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.fields]]
                name = "note"
                type = "string"
                required = false
            "#,
        )
        .expect("meta should load");
        let linked_meta = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        )]);

        assert!(validate_data_set(&linked_meta, &data_set).is_ok());
    }

    #[test]
    fn validates_enum_values() {
        let enum_meta = EnumMeta {
            name: "ItemRarity".to_owned(),
            desc: None,
            values: vec![EnumValueMeta {
                name: "common".to_owned(),
                desc: None,
            }],
        };
        let meta = TableMeta {
            name: "items".to_owned(),
            kind: crate::meta::TableKind::Collection,
            desc: None,
            source: Some("items.csv".to_owned()),
            sources: vec![],
            override_sources: vec![],
            fields: vec![FieldMeta {
                name: "rarity".to_owned(),
                desc: None,
                field_type: "ItemRarity".to_owned(),
                required: true,
                default: None,
                layout: None,
                export: true,
                tool_only: false,
                filter_field: false,
                validator_only: false,
                field_number: None,
                unique: false,
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
                linked_field: LinkedField::Enum(&enum_meta),
            }],
            vec![],
        );
        let valid_table = DataTable {
            name: "items".to_owned(),
            rows: vec![BTreeMap::from([("rarity".to_owned(), "common".to_owned())])],
        };
        let invalid_table = DataTable {
            name: "items".to_owned(),
            rows: vec![BTreeMap::from([(
                "rarity".to_owned(),
                "legendary".to_owned(),
            )])],
        };

        assert!(validate_table(&linked_meta, &valid_table).is_ok());
        assert!(validate_table(&linked_meta, &invalid_table).is_err());
    }

    #[test]
    fn rejects_struct_field_even_when_empty() {
        let struct_meta = crate::meta::StructMeta {
            name: "Reward".to_owned(),
            desc: None,
            fields: Vec::new(),
        };
        let meta = TableMeta {
            name: "items".to_owned(),
            kind: crate::meta::TableKind::Collection,
            desc: None,
            source: Some("items.csv".to_owned()),
            sources: vec![],
            override_sources: vec![],
            fields: vec![FieldMeta {
                name: "reward".to_owned(),
                desc: None,
                field_type: "Reward".to_owned(),
                required: false,
                default: None,
                layout: None,
                export: true,
                tool_only: false,
                filter_field: false,
                validator_only: false,
                field_number: None,
                unique: false,
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
                linked_field: LinkedField::Struct(&struct_meta),
            }],
            vec![],
        );
        let table = DataTable {
            name: "items".to_owned(),
            rows: vec![BTreeMap::from([("reward".to_owned(), String::new())])],
        };
        let error = validate_table(&linked_meta, &table).unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("struct value validation is not implemented yet"))
        );
    }

    #[test]
    fn validates_complete_data_set() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
            "#,
        )
        .expect("meta should load");
        let linked_meta = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        )]);

        assert!(validate_data_set(&linked_meta, &data_set).is_ok());
    }

    #[test]
    fn rejects_data_set_missing_required_table() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
            "#,
        )
        .expect("meta should load");
        let linked_meta = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![]);
        let error = validate_data_set(&linked_meta, &data_set).unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("missing table `items`"))
        );
    }

    #[test]
    fn normalizes_data_set_before_validation() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "max_stack"
                type = "int"
                required = true
                default = 1
            "#,
        )
        .expect("meta should load");
        let linked_meta = meta.link().expect("meta should link");
        let data_set = DataSet::new(vec![DataTable::new("items", vec![BTreeMap::new()])]);

        let normalized = normalize_data_set(&linked_meta, &data_set).expect("normalize data set");

        assert!(validate_normalized_data_set(&linked_meta, &normalized).is_ok());
        assert_eq!(
            normalized.table("items").unwrap().rows[0]
                .get("max_stack")
                .map(String::as_str),
            Some("1")
        );
    }
}
