// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    Result,
    data::{
        ArrayElementValue, ColorValue, DataResolver, DataTable, DataValue, ResolvedValue,
        ScalarValue, VectorValue,
    },
    meta::{
        EnumMeta, LinkedField, LinkedFieldMeta, LinkedFragmentMeta, LinkedStructType,
        LinkedTableMeta, PrimitiveFieldType, TableKind, TargetTags,
    },
    runtime_contract::binary_schema_revision,
    validation::key_policy::target_visible_row_indices,
};

use super::wire::{
    BinaryWriteTarget, FORMAT_VERSION, HEADER_SIZE, MAGIC, PresenceSlot, binary_value_error,
    ensure_javascript_safe_int, expected_scalar, fragment_present, write_bytes,
    write_optional_bitmap, write_varuint, zigzag,
};

pub(super) struct BinaryEncoder<'a, 'meta, W: BinaryWriteTarget> {
    table_meta: &'a LinkedTableMeta<'meta>,
    target_tags: &'a TargetTags,
    out: &'a mut W,
}

impl<'a, 'meta, W: BinaryWriteTarget> BinaryEncoder<'a, 'meta, W> {
    pub(super) fn new(
        table_meta: &'a LinkedTableMeta<'meta>,
        target_tags: &'a TargetTags,
        out: &'a mut W,
    ) -> Self {
        Self {
            table_meta,
            target_tags,
            out,
        }
    }

    pub(super) fn write_table(&mut self, table: &DataTable) -> Result<()> {
        self.write_header()?;
        match self.table_meta.meta.kind {
            TableKind::Collection => self.write_collection(table),
            TableKind::Object => self.write_object(table),
        }
    }

    fn write_header(&mut self) -> Result<()> {
        self.out.write_binary_bytes(MAGIC)?;
        self.out.write_binary_bytes(&FORMAT_VERSION.to_le_bytes())?;
        self.out.write_binary_bytes(&HEADER_SIZE.to_le_bytes())?;
        self.out.write_binary_bytes(
            &binary_schema_revision(self.table_meta, self.target_tags).to_le_bytes(),
        )
    }

    fn write_collection(&mut self, table: &DataTable) -> Result<()> {
        let visible_row_indices =
            target_visible_row_indices(self.table_meta, table, self.target_tags);
        let resolved_table = DataResolver::resolve_table_rows(
            self.table_meta,
            table,
            Some(self.target_tags),
            visible_row_indices.iter().copied(),
        )?;
        write_varuint(self.out, visible_row_indices.len() as u64)?;
        for (resolved_row_index, row_index) in visible_row_indices.iter().enumerate() {
            let resolved_row = resolved_table.rows.get(resolved_row_index).ok_or_else(|| {
                binary_value_error(
                    None::<&str>,
                    format!(
                        "resolved table `{}` is missing row {}",
                        self.table_meta.meta.name,
                        resolved_row_index + 1
                    ),
                    format!("resolved row {}", resolved_row_index + 1),
                    "resolved row for every exported source row",
                    "Check data normalization and target row filtering before binary export.",
                )
            })?;
            self.write_table_record(
                resolved_row,
                table.row_source(*row_index).map(|source| &source.fragments),
            )?;
        }
        Ok(())
    }

    fn write_object(&mut self, table: &DataTable) -> Result<()> {
        if table.rows.len() != 1 {
            return Err(binary_value_error(
                None::<&str>,
                format!(
                    "object table `{}` must normalize to exactly one logical row, got {}",
                    self.table_meta.meta.name,
                    table.rows.len()
                ),
                table.rows.len().to_string(),
                "exactly one normalized object-table row",
                "Normalize and validate object table input before binary export.",
            ));
        }

        if target_visible_row_indices(self.table_meta, table, self.target_tags).is_empty() {
            if self.has_visible_slots() {
                return Err(binary_value_error(
                    None::<&str>,
                    format!(
                        "binary export object table `{}` row is filtered out but visible fields/fragments require a singleton record",
                        self.table_meta.meta.name
                    ),
                    "filtered object row",
                    "target-visible singleton object row",
                    "Align row_export rules with visible object-table fields/fragments for this target.",
                ));
            }
            return Ok(());
        }
        let resolved_table =
            DataResolver::resolve_table(self.table_meta, table, Some(self.target_tags))?;
        let resolved_row = resolved_table.rows.first().ok_or_else(|| {
            binary_value_error(
                None::<&str>,
                format!(
                    "resolved object table `{}` is missing singleton row",
                    self.table_meta.meta.name
                ),
                "no resolved singleton row",
                "resolved singleton row",
                "Check object table normalization and resolver output before binary export.",
            )
        })?;
        if self.has_visible_slots() {
            self.write_table_record(resolved_row, None)?;
        }
        Ok(())
    }

    fn has_visible_slots(&self) -> bool {
        self.table_meta
            .fields
            .iter()
            .any(|field| field.meta.is_exported_for(self.target_tags))
            || self
                .table_meta
                .fragments
                .iter()
                .any(|fragment| fragment.meta.is_exported_for(self.target_tags))
    }

    fn write_table_record(
        &mut self,
        values: &BTreeMap<String, ResolvedValue>,
        active_fragments: Option<&Vec<String>>,
    ) -> Result<()> {
        let fields = self
            .table_meta
            .fields
            .iter()
            .filter(|field| field.meta.is_exported_for(self.target_tags))
            .collect::<Vec<_>>();
        let fragments = self
            .table_meta
            .fragments
            .iter()
            .filter(|fragment| fragment.meta.is_exported_for(self.target_tags))
            .collect::<Vec<_>>();
        let presence_slots = fields
            .iter()
            .map(|field| PresenceSlot::field(field, values))
            .chain(
                fragments
                    .iter()
                    .map(|fragment| PresenceSlot::fragment(fragment, values, active_fragments)),
            );
        write_optional_bitmap(self.out, presence_slots)?;

        for field in fields {
            match values.get(&field.meta.name) {
                Some(value) => self.write_field_value(field, value)?,
                None if field.meta.required => {
                    return Err(binary_value_error(
                        Some(field.meta.name.as_str()),
                        format!(
                            "binary export missing required field `{}.{}`",
                            self.table_meta.meta.name, field.meta.name
                        ),
                        "missing field value",
                        "required resolved field value",
                        "Ensure validation supplies required field values before binary export.",
                    ));
                }
                None => {}
            }
        }
        for fragment in fragments {
            if !fragment_present(fragment, values, active_fragments) {
                continue;
            }
            let value = values.get(&fragment.meta.name).ok_or_else(|| {
                binary_value_error(
                    Some(fragment.meta.name.as_str()),
                    format!(
                        "binary export missing present fragment `{}.{}`",
                        self.table_meta.meta.name, fragment.meta.name
                    ),
                    "missing fragment value",
                    "resolved fragment value when fragment is present",
                    "Ensure active fragments are resolved before binary export.",
                )
            })?;
            self.write_fragment_record(fragment, value)?;
        }
        Ok(())
    }

    fn write_fragment_record(
        &mut self,
        fragment: &LinkedFragmentMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Struct(children) = &value.value else {
            return Err(binary_value_error(
                Some(fragment.meta.name.as_str()),
                format!(
                    "binary export expected struct value for fragment `{}`",
                    fragment.meta.name
                ),
                value_shape(value),
                "struct fragment value",
                "Ensure resolved fragment payloads match the linked fragment shape.",
            ));
        };
        self.write_record(
            &fragment.fields,
            children,
            &format!("fragment `{}`", fragment.meta.name),
        )
    }

    fn write_record(
        &mut self,
        fields: &[LinkedFieldMeta<'meta>],
        values: &BTreeMap<String, ResolvedValue>,
        context: &str,
    ) -> Result<()> {
        let visible_fields = fields
            .iter()
            .filter(|field| field.meta.is_exported_for(self.target_tags))
            .collect::<Vec<_>>();
        write_optional_bitmap(
            self.out,
            visible_fields
                .iter()
                .map(|field| PresenceSlot::field(field, values)),
        )?;
        for field in visible_fields {
            match values.get(&field.meta.name) {
                Some(value) => self.write_field_value(field, value)?,
                None if field.meta.required => {
                    return Err(binary_value_error(
                        Some(field.meta.name.as_str()),
                        format!(
                            "binary export missing required field `{}` in {}",
                            field.meta.name, context
                        ),
                        "missing field value",
                        "required resolved field value",
                        "Ensure validation supplies required nested field values before binary export.",
                    ));
                }
                None => {}
            }
        }
        Ok(())
    }

    fn write_field_value(
        &mut self,
        field: &LinkedFieldMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        match &field.linked_field {
            LinkedField::Primitive(primitive) => self.write_primitive(*primitive, field, value),
            LinkedField::Enum(enum_meta) => self.write_enum(enum_meta, field, value),
            LinkedField::Struct(struct_type) => self.write_struct(struct_type, value),
            LinkedField::List(element_type) => self.write_list(element_type, value),
        }
    }

    fn write_list(
        &mut self,
        element_type: &LinkedField<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Array(elements) = &value.value else {
            return Err(binary_value_error(
                None::<&str>,
                "binary export expected list value".to_owned(),
                value_shape(value),
                "list runtime value",
                "Ensure resolved data field values match the linked list field type before binary export.",
            ));
        };
        write_varuint(self.out, elements.len() as u64)?;
        for element in elements {
            self.write_array_element_for_type(element_type, element)?;
        }
        Ok(())
    }

    fn write_array_element_for_type(
        &mut self,
        field_type: &LinkedField<'meta>,
        value: &ArrayElementValue,
    ) -> Result<()> {
        match field_type {
            LinkedField::Primitive(primitive) => {
                self.write_primitive_array_element(*primitive, value)
            }
            LinkedField::Enum(enum_meta) => self.write_enum_array_element(enum_meta, value),
            LinkedField::Struct(struct_type) => self.write_struct_array_element(struct_type, value),
            LinkedField::List(element_type) => {
                self.write_nested_list_array_element(element_type, value)
            }
        }
    }

    fn write_primitive(
        &mut self,
        primitive: PrimitiveFieldType,
        field: &LinkedFieldMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        if matches!(primitive, PrimitiveFieldType::Color) {
            let DataValue::Color(color) = &value.value else {
                return Err(binary_value_error(
                    Some(field.meta.name.as_str()),
                    format!(
                        "binary export field `{}` expected color value",
                        field.meta.name
                    ),
                    value_shape(value),
                    "color runtime value",
                    "Ensure resolved data field values match the linked color field type before binary export.",
                ));
            };
            return self.write_color(color);
        }
        if primitive.vector_dimension().is_some() {
            let DataValue::Vector(vector) = &value.value else {
                return Err(binary_value_error(
                    Some(field.meta.name.as_str()),
                    format!(
                        "binary export field `{}` expected vector value",
                        field.meta.name
                    ),
                    value_shape(value),
                    "vector runtime value",
                    "Ensure resolved data field values match the linked vector field type before binary export.",
                ));
            };
            return self.write_vector(vector);
        }
        let DataValue::Scalar(scalar) = &value.value else {
            return Err(expected_scalar(field));
        };
        self.write_scalar(primitive, scalar, field.meta.name.as_str())
    }

    fn write_color(&mut self, color: &ColorValue) -> Result<()> {
        for component in [color.r, color.g, color.b, color.a] {
            self.out.write_binary_bytes(&component.to_le_bytes())?;
        }
        Ok(())
    }

    fn write_vector(&mut self, vector: &VectorValue) -> Result<()> {
        match vector {
            VectorValue::Float(values) => {
                for component in values {
                    self.out.write_binary_bytes(&component.to_le_bytes())?;
                }
            }
            VectorValue::Int(values) => {
                for component in values {
                    self.out
                        .write_binary_bytes(&(*component as f64).to_le_bytes())?;
                }
            }
        }
        Ok(())
    }

    fn write_primitive_array_element(
        &mut self,
        primitive: PrimitiveFieldType,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Scalar(scalar) = &value.value else {
            return Err(binary_value_error(
                None::<&str>,
                "binary export expected scalar list element".to_owned(),
                array_element_shape(value),
                "scalar list element",
                "Ensure resolved list elements match the linked primitive element type before binary export.",
            ));
        };
        self.write_scalar(primitive, scalar, "<list element>")
    }

    fn write_scalar(
        &mut self,
        primitive: PrimitiveFieldType,
        scalar: &ScalarValue,
        field_name: &str,
    ) -> Result<()> {
        match (primitive, scalar) {
            (PrimitiveFieldType::Int, ScalarValue::Int(value)) => {
                ensure_javascript_safe_int(field_name, *value)?;
                write_varuint(self.out, zigzag(*value))?;
                Ok(())
            }
            (PrimitiveFieldType::Float, ScalarValue::Float(value)) => {
                if !value.is_finite() {
                    return Err(binary_value_error(
                        Some(field_name),
                        format!(
                            "binary export field `{}` contains non-finite float `{}`",
                            field_name, value
                        ),
                        value.to_string(),
                        "finite float value",
                        "Use a finite numeric value for binary runtime export.",
                    ));
                }
                self.out.write_binary_bytes(&value.to_le_bytes())?;
                Ok(())
            }
            (PrimitiveFieldType::Bool, ScalarValue::Bool(value)) => {
                self.out.write_binary_bytes(&[u8::from(*value)])?;
                Ok(())
            }
            (
                PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
                ScalarValue::String(value),
            ) => {
                write_bytes(self.out, value.as_bytes())?;
                Ok(())
            }
            _ => Err(binary_value_error(
                Some(field_name),
                format!(
                    "binary export field `{}` has scalar incompatible with `{}`",
                    field_name,
                    primitive.as_str()
                ),
                scalar_shape(scalar),
                format!("{} scalar", primitive.as_str()),
                "Ensure resolved scalar values match the linked primitive field type before binary export.",
            )),
        }
    }

    fn write_enum(
        &mut self,
        enum_meta: &EnumMeta,
        field: &LinkedFieldMeta<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Scalar(ScalarValue::Enum(value)) = &value.value else {
            return Err(expected_scalar(field));
        };
        self.write_enum_value(enum_meta, value, field.meta.name.as_str())
    }

    fn write_enum_array_element(
        &mut self,
        enum_meta: &EnumMeta,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Scalar(ScalarValue::Enum(value)) = &value.value else {
            return Err(binary_value_error(
                None::<&str>,
                "binary export expected enum list element".to_owned(),
                array_element_shape(value),
                "enum list element",
                "Ensure resolved list elements match the linked enum element type before binary export.",
            ));
        };
        self.write_enum_value(enum_meta, value, "<list element>")
    }

    fn write_enum_value(
        &mut self,
        enum_meta: &EnumMeta,
        value: &str,
        field_name: &str,
    ) -> Result<()> {
        let ordinal = enum_meta
            .values
            .iter()
            .position(|enum_value| enum_value.name == value)
            .ok_or_else(|| {
                binary_value_error(
                    Some(field_name),
                    format!(
                        "binary export field `{}` value `{}` is not a member of enum `{}`",
                        field_name, value, enum_meta.name
                    ),
                    value.to_string(),
                    format!("member of enum `{}`", enum_meta.name),
                    "Ensure enum values are validated against the linked enum before binary export.",
                )
            })?;
        write_varuint(self.out, ordinal as u64)?;
        Ok(())
    }

    fn write_struct(
        &mut self,
        struct_type: &LinkedStructType<'meta>,
        value: &ResolvedValue,
    ) -> Result<()> {
        let DataValue::Struct(children) = &value.value else {
            return Err(binary_value_error(
                None::<&str>,
                format!(
                    "binary export expected struct `{}` value",
                    struct_type.meta.name
                ),
                value_shape(value),
                format!("struct `{}` value", struct_type.meta.name),
                "Ensure resolved data field values match the linked struct field type before binary export.",
            ));
        };
        self.write_record(
            &struct_type.fields,
            children,
            &format!("struct `{}`", struct_type.meta.name),
        )
    }

    fn write_struct_array_element(
        &mut self,
        struct_type: &LinkedStructType<'meta>,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Struct(children) = &value.value else {
            return Err(binary_value_error(
                None::<&str>,
                format!(
                    "binary export expected struct `{}` list element",
                    struct_type.meta.name
                ),
                array_element_shape(value),
                format!("struct `{}` list element", struct_type.meta.name),
                "Ensure resolved list elements match the linked struct element type before binary export.",
            ));
        };
        self.write_record(
            &struct_type.fields,
            children,
            &format!("struct `{}` list element", struct_type.meta.name),
        )
    }

    fn write_nested_list_array_element(
        &mut self,
        element_type: &LinkedField<'meta>,
        value: &ArrayElementValue,
    ) -> Result<()> {
        let DataValue::Array(elements) = &value.value else {
            return Err(binary_value_error(
                None::<&str>,
                "binary export expected nested list element".to_owned(),
                array_element_shape(value),
                "nested list element",
                "Ensure resolved list elements match the linked nested list element type before binary export.",
            ));
        };
        write_varuint(self.out, elements.len() as u64)?;
        for element in elements {
            self.write_array_element_for_type(element_type, element)?;
        }
        Ok(())
    }
}

fn value_shape(value: &ResolvedValue) -> &'static str {
    data_value_shape(&value.value)
}

fn array_element_shape(value: &ArrayElementValue) -> &'static str {
    data_value_shape(&value.value)
}

fn data_value_shape(value: &DataValue) -> &'static str {
    match value {
        DataValue::Scalar(_) => "scalar runtime value",
        DataValue::Color(_) => "color runtime value",
        DataValue::Vector(_) => "vector runtime value",
        DataValue::Struct(_) => "struct runtime value",
        DataValue::Array(_) => "list runtime value",
    }
}

fn scalar_shape(value: &ScalarValue) -> &'static str {
    match value {
        ScalarValue::Int(_) => "int scalar",
        ScalarValue::Float(_) => "float scalar",
        ScalarValue::Bool(_) => "bool scalar",
        ScalarValue::String(_) => "string scalar",
        ScalarValue::Enum(_) => "enum scalar",
    }
}
