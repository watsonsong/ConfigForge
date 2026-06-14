// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::{CodeField, CodeFragment, CodeTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReaderShapePlan<'a> {
    pub(crate) fields: &'a [CodeField],
    pub(crate) fragments: &'a [CodeFragment],
    pub(crate) optional_slot_count: usize,
}

impl<'a> ReaderShapePlan<'a> {
    pub(crate) fn for_fields(fields: &'a [CodeField]) -> Self {
        Self::new(fields, &[])
    }

    pub(crate) fn for_table(table: &'a CodeTable) -> Self {
        Self::new(&table.fields, &table.fragments)
    }

    fn new(fields: &'a [CodeField], fragments: &'a [CodeFragment]) -> Self {
        Self {
            fields,
            fragments,
            optional_slot_count: fields.iter().filter(|field| !field.required).count()
                + fragments.len(),
        }
    }
}
