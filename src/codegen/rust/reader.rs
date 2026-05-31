// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    codegen::{
        CodeEnum, CodeField, CodeFragment, CodeStruct, CodeTable, CodeWriter,
        ir::CodeRelationCardinality,
    },
};

use super::types::RustNames;

pub(super) fn render_enum_reader_impl(
    content: &mut CodeWriter,
    code_enum: &CodeEnum,
    names: &RustNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "impl {} {{\n",
        names.type_name(&code_enum.name)?
    ));
    content.raw(
        "    fn read_from_reader<R: RuntimeDataReader>(reader: &R) -> RuntimeDataResult<Self> {\n",
    );
    content.raw("        match reader.read_enum_index(&[\n");
    for value in &code_enum.values {
        content.raw_fmt(format_args!("            {:?},\n", value.name));
    }
    content.raw("        ])? {\n");
    for (ordinal, value) in code_enum.values.iter().enumerate() {
        content.raw_fmt(format_args!(
            "            {ordinal} => Ok(Self::{}),\n",
            names.enum_variant_name(&value.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "            other => Err(RuntimeDataError::decode(format!(\"unknown {} enum index `{{other}}`\"))),\n",
        code_enum.name
    ));
    content.raw("        }\n");
    content.raw("    }\n");
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_struct_reader_impl(
    content: &mut CodeWriter,
    code_struct: &CodeStruct,
    names: &RustNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "impl {} {{\n",
        names.type_name(&code_struct.name)?
    ));
    render_read_from_reader_body(content, &code_struct.fields, None, &code_struct.name, names)?;
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_fragment_reader_impl(
    content: &mut CodeWriter,
    table: &CodeTable,
    fragment: &CodeFragment,
    names: &RustNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "impl {} {{\n",
        names.fragment_type_name(&table.name, &fragment.name)?
    ));
    render_read_from_reader_body(content, &fragment.fields, None, &fragment.name, names)?;
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_row_reader_impl(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &RustNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "impl {} {{\n",
        names.row_type_name(&table.name)?
    ));
    render_read_from_reader_body(content, &table.fields, Some(table), &table.name, names)?;
    content.raw("}\n\n");
    Ok(())
}

fn render_read_from_reader_body(
    content: &mut CodeWriter,
    fields: &[CodeField],
    table: Option<&CodeTable>,
    type_name: &str,
    names: &RustNames<'_>,
) -> Result<()> {
    let fragment_count = table.map(|table| table.fragments.len()).unwrap_or(0);
    let optional_slot_count =
        fields.iter().filter(|field| !field.required).count() + fragment_count;
    content.raw(
        "    fn read_from_reader<R: RuntimeDataReader>(reader: &R) -> RuntimeDataResult<Self> {\n",
    );
    content.raw_fmt(format_args!(
        "        reader.begin_record({optional_slot_count})?;\n"
    ));
    content.raw("        let value = Self {\n");
    for field in fields {
        content.raw_fmt(format_args!(
            "            {}: {},\n",
            names.field_name(&field.name)?,
            names.rust_read_field_expr(field)?
        ));
    }
    if let Some(table) = table {
        for fragment in &table.fragments {
            let field_name = names.field_name(&fragment.name)?;
            let payload_type = names.fragment_payload_type_name(table, fragment)?;
            content.raw_fmt(format_args!(
                concat!(
                    "            {field_name}: with_optional_field(reader, {:?}, {payload_type}::read_from_reader)?,\n",
                ),
                fragment.name,
                field_name = field_name,
                payload_type = payload_type
            ));
        }
        for foreign_key in table
            .foreign_keys
            .iter()
            .filter(|foreign_key| foreign_key.linked_field.is_some())
        {
            let cache_init = match foreign_key
                .linked_field
                .as_ref()
                .expect("linked field")
                .cardinality
            {
                CodeRelationCardinality::Many => "RefCell::new(LinkListCache::default())",
                _ => "Cell::new(LinkCache::default())",
            };
            content.raw_fmt(format_args!(
                "            {}: {},\n",
                names.relation_cache_field_name(foreign_key)?,
                cache_init
            ));
        }
    }
    content.raw("        };\n");
    content.raw("        reader.end_record()?;\n");
    content.raw("        Ok(value)\n");
    content.raw("    }\n");
    if fields.is_empty()
        && table
            .map(|table| table.fragments.is_empty())
            .unwrap_or(true)
    {
        content.raw_fmt(format_args!(
            "    // `{type_name}` has no runtime fields.\n"
        ));
    }
    Ok(())
}
