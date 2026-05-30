// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    meta::{LinkedField, LinkedFieldMeta, PrimitiveFieldType},
};

use super::{
    DataValue, ResolvedValue, RowSourceInfo, ScalarValue, ValueSourceKind,
    value_conversion::json_value_to_scalar_cell_for_field,
};

pub(crate) fn parse_scalar_field_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    value: &str,
) -> Result<ScalarValue> {
    parse_scalar_for_type(
        table_name,
        row_number,
        field_path,
        &field.linked_field,
        value,
    )
    .map_err(|_| scalar_type_error(table_name, row_number, field_path, field, value))
}

pub(crate) fn parse_scalar_for_type(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    linked_field: &LinkedField<'_>,
    value: &str,
) -> Result<ScalarValue> {
    match linked_field {
        LinkedField::Primitive(PrimitiveFieldType::Int) => value
            .parse::<i64>()
            .map(ScalarValue::Int)
            .map_err(|_| scalar_value_parse_error(table_name, row_number, field_path, value)),
        LinkedField::Primitive(PrimitiveFieldType::Float) => value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(ScalarValue::Float)
            .ok_or_else(|| scalar_value_parse_error(table_name, row_number, field_path, value)),
        LinkedField::Primitive(PrimitiveFieldType::Bool) => match value {
            "true" | "1" => Ok(ScalarValue::Bool(true)),
            "false" | "0" => Ok(ScalarValue::Bool(false)),
            _ => Err(scalar_value_parse_error(
                table_name, row_number, field_path, value,
            )),
        },
        LinkedField::Primitive(
            PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
        ) => Ok(ScalarValue::String(value.to_owned())),
        LinkedField::Enum(enum_meta) => {
            if enum_meta
                .values
                .iter()
                .any(|enum_value| enum_value.name == value)
            {
                Ok(ScalarValue::Enum(value.to_owned()))
            } else {
                Err(scalar_value_parse_error(
                    table_name, row_number, field_path, value,
                ))
            }
        }
        LinkedField::Struct(_) => Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` expected scalar field, got struct type",
            table_name, row_number, field_path
        ))),
        LinkedField::List(_) => Err(ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` expected scalar field, got list type",
            table_name, row_number, field_path
        ))),
    }
}

pub(crate) fn parse_list_cell_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    element_type: &LinkedField<'_>,
    value: &str,
    row_source: Option<&RowSourceInfo>,
    source_kind: ValueSourceKind,
) -> Result<Vec<ResolvedValue>> {
    let json: serde_json::Value = serde_json::from_str(value).map_err(|source| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` has invalid list JSON: {}",
            table_name, row_number, field_path, source
        ))
    })?;
    parse_list_json_value(
        table_name,
        row_number,
        field_path,
        element_type,
        &json,
        row_source,
        source_kind,
    )
}

pub(crate) fn parse_list_json_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    element_type: &LinkedField<'_>,
    value: &serde_json::Value,
    row_source: Option<&RowSourceInfo>,
    source_kind: ValueSourceKind,
) -> Result<Vec<ResolvedValue>> {
    let array = value.as_array().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` must be a JSON array",
            table_name, row_number, field_path
        ))
    })?;
    array
        .iter()
        .enumerate()
        .map(|(index, element)| {
            let element_path = format!("{field_path}[{index}]");
            let raw = json_value_to_scalar_cell_for_field(
                element,
                table_name,
                row_number,
                &element_path,
            )?;
            let scalar = parse_scalar_for_type(
                table_name,
                row_number,
                &element_path,
                element_type,
                &raw,
            )
            .map_err(|_| {
                ConfigForgeError::Validation(format!(
                    "table `{}` row {} field `{}` value `{}` does not match list element type",
                    table_name, row_number, element_path, raw
                ))
            })?;
            Ok(ResolvedValue::from_source_kind(
                element_path,
                DataValue::Scalar(scalar),
                source_kind,
                Some(raw),
                row_source,
            ))
        })
        .collect()
}

fn scalar_value_parse_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    value: &str,
) -> ConfigForgeError {
    ConfigForgeError::Validation(format!(
        "table `{}` row {} field `{}` value `{}` does not match scalar type",
        table_name, row_number, field_path, value
    ))
}

fn scalar_type_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    value: &str,
) -> ConfigForgeError {
    ConfigForgeError::Validation(format!(
        "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
        table_name, row_number, field_path, value, field.meta.field_type
    ))
}

#[cfg(test)]
mod tests {
    use crate::meta::{EnumMeta, EnumValueMeta};

    use super::*;

    #[test]
    fn parses_scalar_values_for_primitive_and_enum_types() {
        let enum_meta = EnumMeta {
            name: "Rarity".to_owned(),
            desc: None,
            values: vec![EnumValueMeta {
                name: "rare".to_owned(),
                desc: None,
            }],
        };

        assert_eq!(
            parse_scalar_for_type(
                "items",
                1,
                "id",
                &LinkedField::Primitive(PrimitiveFieldType::Int),
                "42",
            )
            .unwrap(),
            ScalarValue::Int(42)
        );
        assert_eq!(
            parse_scalar_for_type("items", 1, "rarity", &LinkedField::Enum(&enum_meta), "rare")
                .unwrap(),
            ScalarValue::Enum("rare".to_owned())
        );
        assert!(
            parse_scalar_for_type("items", 1, "rarity", &LinkedField::Enum(&enum_meta), "bad")
                .is_err()
        );
    }

    #[test]
    fn parses_json_list_elements_once_with_element_paths() {
        let values = parse_list_cell_value(
            "items",
            7,
            "starter_item_ids",
            &LinkedField::Primitive(PrimitiveFieldType::Int),
            "[2001,2002]",
            None,
            ValueSourceKind::ExplicitCell,
        )
        .unwrap();

        assert_eq!(values.len(), 2);
        assert_eq!(values[0].source.raw_field_path, "starter_item_ids[0]");
        assert_eq!(values[0].value, DataValue::Scalar(ScalarValue::Int(2001)));
        assert_eq!(values[1].source.raw_field_path, "starter_item_ids[1]");
        assert_eq!(values[1].value, DataValue::Scalar(ScalarValue::Int(2002)));
    }

    #[test]
    fn parses_empty_and_enum_json_lists() {
        let empty = parse_list_cell_value(
            "items",
            7,
            "starter_item_ids",
            &LinkedField::Primitive(PrimitiveFieldType::Int),
            "[]",
            None,
            ValueSourceKind::ExplicitCell,
        )
        .unwrap();
        assert!(empty.is_empty());

        let enum_meta = EnumMeta {
            name: "Rarity".to_owned(),
            desc: None,
            values: vec![
                EnumValueMeta {
                    name: "common".to_owned(),
                    desc: None,
                },
                EnumValueMeta {
                    name: "rare".to_owned(),
                    desc: None,
                },
            ],
        };
        let values = parse_list_cell_value(
            "items",
            7,
            "rarities",
            &LinkedField::Enum(&enum_meta),
            r#"["common","rare"]"#,
            None,
            ValueSourceKind::ExplicitCell,
        )
        .unwrap();

        assert_eq!(values.len(), 2);
        assert_eq!(
            values[0].value,
            DataValue::Scalar(ScalarValue::Enum("common".to_owned()))
        );
        assert_eq!(
            values[1].value,
            DataValue::Scalar(ScalarValue::Enum("rare".to_owned()))
        );
    }

    #[test]
    fn rejects_invalid_or_non_array_list_json() {
        let invalid = parse_list_cell_value(
            "items",
            7,
            "starter_item_ids",
            &LinkedField::Primitive(PrimitiveFieldType::Int),
            "not json",
            None,
            ValueSourceKind::ExplicitCell,
        )
        .unwrap_err();
        assert!(
            matches!(invalid, ConfigForgeError::Validation(message) if message.contains("invalid list JSON"))
        );

        let non_array = parse_list_cell_value(
            "items",
            7,
            "starter_item_ids",
            &LinkedField::Primitive(PrimitiveFieldType::Int),
            "{}",
            None,
            ValueSourceKind::ExplicitCell,
        )
        .unwrap_err();
        assert!(
            matches!(non_array, ConfigForgeError::Validation(message) if message.contains("must be a JSON array"))
        );
    }
}
