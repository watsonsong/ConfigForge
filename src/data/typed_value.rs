// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{CF_DATA_INVALID_ENUM, CF_DATA_INVALID_TYPE},
    meta::{LinkedField, LinkedFieldMeta, PrimitiveFieldType},
};

use super::{
    ArrayElementValue, RowSourceInfo, ScalarValue, ValueSourceKind,
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
                Err(enum_value_parse_error(
                    table_name, row_number, field_path, enum_meta, value,
                ))
            }
        }
        LinkedField::Struct(_) => Err(scalar_value_type_error(
            table_name,
            row_number,
            field_path,
            "struct type",
            "scalar field type",
            format!(
                "table `{}` row {} field `{}` expected scalar field, got struct type",
                table_name, row_number, field_path
            ),
        )),
        LinkedField::List(_) => Err(scalar_value_type_error(
            table_name,
            row_number,
            field_path,
            "list type",
            "scalar field type",
            format!(
                "table `{}` row {} field `{}` expected scalar field, got list type",
                table_name, row_number, field_path
            ),
        )),
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
) -> Result<Vec<ArrayElementValue>> {
    let json: serde_json::Value = serde_json::from_str(value).map_err(|source| {
        list_json_error(
            table_name,
            row_number,
            field_path,
            value,
            "valid JSON array",
            format!(
                "table `{}` row {} field `{}` has invalid list JSON: {}",
                table_name, row_number, field_path, source
            ),
        )
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
    _row_source: Option<&RowSourceInfo>,
    _source_kind: ValueSourceKind,
) -> Result<Vec<ArrayElementValue>> {
    let array = value.as_array().ok_or_else(|| {
        list_json_error(
            table_name,
            row_number,
            field_path,
            value.to_string(),
            "JSON array",
            format!(
                "table `{}` row {} field `{}` must be a JSON array",
                table_name, row_number, field_path
            ),
        )
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
                scalar_value_type_error(
                    table_name,
                    row_number,
                    &element_path,
                    raw.clone(),
                    format!("list element type `{}`", expected_type_name(element_type)),
                    format!(
                        "table `{}` row {} field `{}` value `{}` does not match list element type",
                        table_name, row_number, element_path, raw
                    ),
                )
            })?;
            Ok(ArrayElementValue::scalar(scalar, Some(raw)))
        })
        .collect()
}

fn scalar_value_parse_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    value: &str,
) -> ConfigForgeError {
    scalar_value_type_error(
        table_name,
        row_number,
        field_path,
        value,
        "scalar type",
        format!(
            "table `{}` row {} field `{}` value `{}` does not match scalar type",
            table_name, row_number, field_path, value
        ),
    )
}

fn scalar_type_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    value: &str,
) -> ConfigForgeError {
    scalar_value_type_error(
        table_name,
        row_number,
        field_path,
        value,
        format!("{:?}", field.meta.field_type),
        format!(
            "table `{}` row {} field `{}` value `{}` does not match type `{:?}`",
            table_name, row_number, field_path, value, field.meta.field_type
        ),
    )
}

fn enum_value_parse_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    enum_meta: &crate::meta::EnumMeta,
    value: &str,
) -> ConfigForgeError {
    let expected = enum_meta
        .values
        .iter()
        .map(|enum_value| enum_value.name.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_INVALID_ENUM,
            DiagnosticSeverity::Error,
            format!(
                "table `{}` row {} field `{}` value `{}` is not a valid `{}` enum value",
                table_name, row_number, field_path, value, enum_meta.name
            ),
            data_value_location(table_name, row_number, field_path),
        )
        .with_source("data.typed_value")
        .with_domain("data")
        .with_current_value(value)
        .with_expected(format!("one of: {expected}"))
        .with_hint(format!("Use a declared `{}` enum value.", enum_meta.name)),
    )
}

fn list_json_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    message: String,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_INVALID_TYPE,
            DiagnosticSeverity::Error,
            message,
            data_value_location(table_name, row_number, field_path),
        )
        .with_source("data.typed_value")
        .with_domain("data")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Use JSON array syntax for list field values."),
    )
}

fn scalar_value_type_error(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    current_value: impl Into<String>,
    expected: impl Into<String>,
    message: String,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_DATA_INVALID_TYPE,
            DiagnosticSeverity::Error,
            message,
            data_value_location(table_name, row_number, field_path),
        )
        .with_source("data.typed_value")
        .with_domain("data")
        .with_current_value(current_value)
        .with_expected(expected)
        .with_hint("Check the cell value against the field type declared in Meta."),
    )
}

fn data_value_location(
    table_name: &str,
    row_number: usize,
    field_path: &str,
) -> DiagnosticLocation {
    DiagnosticLocation {
        table: Some(table_name.to_owned()),
        field: Some(field_path.to_owned()),
        source_row_number: Some(row_number),
        ..DiagnosticLocation::default()
    }
}

fn expected_type_name(field: &LinkedField<'_>) -> String {
    match field {
        LinkedField::Primitive(primitive) => format!("{primitive:?}"),
        LinkedField::Enum(enum_meta) => format!("enum {}", enum_meta.name),
        LinkedField::Struct(struct_meta) => format!("struct {}", struct_meta.meta.name),
        LinkedField::List(_) => "list".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use crate::data::DataValue;
    use crate::diagnostic::code::{CF_DATA_INVALID_ENUM, CF_DATA_INVALID_TYPE};
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
        let invalid_enum =
            parse_scalar_for_type("items", 1, "rarity", &LinkedField::Enum(&enum_meta), "bad")
                .unwrap_err();
        let ConfigForgeError::ValidationReport(report) = invalid_enum else {
            panic!("expected structured enum diagnostic");
        };
        assert_eq!(report.diagnostics[0].code, CF_DATA_INVALID_ENUM);
        assert_eq!(report.diagnostics[0].current_value.as_deref(), Some("bad"));
        assert_eq!(
            report.diagnostics[0].location.table.as_deref(),
            Some("items")
        );
        assert_eq!(
            report.diagnostics[0].location.field.as_deref(),
            Some("rarity")
        );
    }

    #[test]
    fn parses_json_list_elements_without_per_element_source_trace() {
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
        assert_eq!(values[0].value, DataValue::Scalar(ScalarValue::Int(2001)));
        assert_eq!(values[0].raw_value.as_deref(), Some("2001"));
        assert_eq!(values[1].value, DataValue::Scalar(ScalarValue::Int(2002)));
        assert_eq!(values[1].raw_value.as_deref(), Some("2002"));
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
        let ConfigForgeError::ValidationReport(report) = invalid else {
            panic!("expected structured list JSON diagnostic");
        };
        assert_eq!(report.diagnostics[0].code, CF_DATA_INVALID_TYPE);
        assert!(report.diagnostics[0].message.contains("invalid list JSON"));
        assert_eq!(
            report.diagnostics[0].current_value.as_deref(),
            Some("not json")
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
        let ConfigForgeError::ValidationReport(report) = non_array else {
            panic!("expected structured list JSON diagnostic");
        };
        assert_eq!(report.diagnostics[0].code, CF_DATA_INVALID_TYPE);
        assert!(
            report.diagnostics[0]
                .message
                .contains("must be a JSON array")
        );
        assert_eq!(
            report.diagnostics[0].expected.as_deref(),
            Some("JSON array")
        );
    }
}
