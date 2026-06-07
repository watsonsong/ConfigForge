// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#[path = "cli_integration/support.rs"]
mod support;

#[path = "cli_integration/artifact_modes.rs"]
mod artifact_modes;
#[path = "cli_integration/code_only.rs"]
mod code_only;
#[path = "cli_integration/codegen_tasks.rs"]
mod codegen_tasks;
#[path = "cli_integration/export_conflicts.rs"]
mod export_conflicts;
#[path = "cli_integration/fixture_exports.rs"]
mod fixture_exports;
#[path = "cli_integration/resource_flags.rs"]
mod resource_flags;
#[path = "cli_integration/schema_doc.rs"]
mod schema_doc;
#[path = "cli_integration/standard_project.rs"]
mod standard_project;
#[path = "cli_integration/target_filtering.rs"]
mod target_filtering;
#[path = "cli_integration/validation.rs"]
mod validation;
