// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    data::DataTable,
    data_artifact::RuntimeTableFormat,
    meta::{MetaModel, TargetTags},
    runtime_contract::binary_schema_revision,
};

use super::{
    BinaryDataExporter,
    wire::{FORMAT_VERSION, HEADER_SIZE, MAGIC, PresenceSlot, write_optional_bitmap},
};

#[test]
fn optional_bitmap_skips_required_slots_and_packs_optional_presence() {
    let mut bytes = Vec::new();

    write_optional_bitmap(
        &mut bytes,
        [
            PresenceSlot {
                optional: false,
                present: true,
            },
            PresenceSlot {
                optional: true,
                present: true,
            },
            PresenceSlot {
                optional: true,
                present: false,
            },
            PresenceSlot {
                optional: true,
                present: true,
            },
        ],
    )
    .expect("bitmap should write");

    assert_eq!(bytes, vec![0b0000_0101]);
}

#[test]
fn optional_bitmap_extends_to_multiple_bytes() {
    let mut bytes = Vec::new();
    let slots = (0..10).map(|index| PresenceSlot {
        optional: true,
        present: index == 0 || index == 8 || index == 9,
    });

    write_optional_bitmap(&mut bytes, slots).expect("bitmap should write");

    assert_eq!(bytes, vec![0b0000_0001, 0b0000_0011]);
}

#[test]
fn writes_header_and_collection_rows() {
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
                name = "name"
                type = "string"
                required = false
            "#,
    )
    .unwrap();
    let linked = meta.link().unwrap();
    let table_meta = linked.table("items").unwrap();
    let table = DataTable::new(
        "items",
        vec![BTreeMap::from([
            ("id".to_owned(), "7".to_owned()),
            ("name".to_owned(), "Potion".to_owned()),
        ])],
    );

    let bytes = BinaryDataExporter
        .serialize_table(table_meta, &table, &TargetTags::default())
        .unwrap();

    assert_eq!(&bytes[0..4], MAGIC);
    assert_eq!(
        u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
        FORMAT_VERSION
    );
    assert_eq!(
        u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
        HEADER_SIZE
    );
    assert_eq!(
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        binary_schema_revision(table_meta, &TargetTags::default())
    );
    assert_eq!(bytes[16], 1); // row_count
    assert_eq!(bytes[17], 0b0000_0001); // id required, name optional present
    assert_eq!(bytes[18], 14); // zigzag(7)
    assert_eq!(bytes[19], 6);
    assert_eq!(&bytes[20..26], b"Potion");
}
