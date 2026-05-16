use std::collections::HashMap;

use super::{
    FieldMeta, LinkedField, LinkedFieldMeta, LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta,
    LinkedTableMeta, MetaDiagnostic, MetaDiagnosticCode, MetaModel, PrimitiveFieldType,
};

pub fn link_meta_model(model: &MetaModel) -> (Option<LinkedMetaModel<'_>>, Vec<MetaDiagnostic>) {
    let mut diagnostics = Vec::new();
    let enum_index = model
        .enums
        .iter()
        .map(|enum_meta| (enum_meta.name.as_str(), enum_meta))
        .collect::<HashMap<_, _>>();
    let struct_index = model
        .structs
        .iter()
        .map(|struct_meta| (struct_meta.name.as_str(), struct_meta))
        .collect::<HashMap<_, _>>();

    let mut linked_tables = Vec::new();
    for table in &model.tables {
        let field_index = table
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field))
            .collect::<HashMap<_, _>>();
        let linked_fields =
            link_fields(&enum_index, &struct_index, &table.fields, &mut diagnostics);
        let linked_indexes = link_indexes(table, &field_index);
        linked_tables.push(LinkedTableMeta::new(table, linked_fields, linked_indexes));
    }

    let linked_structs = model
        .structs
        .iter()
        .map(|struct_meta| {
            let linked_fields = link_fields(
                &enum_index,
                &struct_index,
                &struct_meta.fields,
                &mut diagnostics,
            );
            LinkedStructMeta::new(struct_meta, linked_fields)
        })
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        (
            Some(LinkedMetaModel::new(model, linked_tables, linked_structs)),
            diagnostics,
        )
    } else {
        (None, diagnostics)
    }
}

impl MetaModel {
    pub fn link(&self) -> std::result::Result<LinkedMetaModel<'_>, Vec<MetaDiagnostic>> {
        let (linked_model, diagnostics) = link_meta_model(self);
        if diagnostics.is_empty() {
            Ok(linked_model.expect("linked model should exist when diagnostics are empty"))
        } else {
            Err(diagnostics)
        }
    }
}

fn link_fields<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    fields: &'meta [FieldMeta],
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Vec<LinkedFieldMeta<'meta>> {
    fields
        .iter()
        .filter_map(|field| link_field_type(enum_index, struct_index, field, diagnostics))
        .collect()
}

fn link_indexes<'meta>(
    table: &'meta super::TableMeta,
    field_index: &HashMap<&'meta str, &'meta FieldMeta>,
) -> Vec<LinkedIndexMeta<'meta>> {
    table
        .indexes
        .iter()
        .map(|index| LinkedIndexMeta {
            meta: index,
            fields: index
                .fields
                .iter()
                .filter_map(|field_name| field_index.get(field_name.as_str()).copied())
                .collect(),
        })
        .collect()
}

fn link_field_type<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    field: &'meta FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Option<LinkedFieldMeta<'meta>> {
    let linked_field = if let Some(primitive) = PrimitiveFieldType::parse(&field.field_type) {
        LinkedField::Primitive(primitive)
    } else if let Some(enum_meta) = enum_index.get(field.field_type.as_str()) {
        LinkedField::Enum(*enum_meta)
    } else if let Some(struct_meta) = struct_index.get(field.field_type.as_str()) {
        LinkedField::Struct(*struct_meta)
    } else {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnknownFieldType,
            format!(
                "field `{}` references unknown type `{}`",
                field.name, field.field_type
            ),
        ));
        return None;
    };

    Some(LinkedFieldMeta {
        meta: field,
        linked_field,
    })
}
