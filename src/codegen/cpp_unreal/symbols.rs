// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::types::CppUnrealNames;
use crate::codegen::CodeModel;

pub(super) fn validate_generated_symbols(
    _model: &CodeModel,
    _names: &CppUnrealNames,
) -> Result<()> {
    Ok(())
}
