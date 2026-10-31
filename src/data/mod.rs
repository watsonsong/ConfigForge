// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod csv;
mod loader;
mod model;
mod plan;
mod provider;
mod resolved;
mod resolver;
pub(crate) mod typed_value;
pub(crate) mod value_conversion;
mod xlsx;

use crate::{ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity};

pub use self::csv::CsvDataReader;
pub use self::loader::DataLoader;
pub use self::model::{
    CellSourceInfo, DataRow, DataSet, DataTable, RawSourceData, RowSourceInfo, SourceRole,
};
pub use self::plan::SourceReadPlan;
pub use self::plan::{DataLoadPlan, DataProvider, DataSourceLayout, DataSourcePlan};
pub use self::provider::{
    AsyncSourceReadShell, LocalCsvSourceProvider, RemoteProviderPolicy, SourceProvider,
    SourceProviderConcurrency, SourceProviderResource,
};
pub use self::resolved::{
    ArrayElementValue, ColorValue, DataValue, ResolvedRow, ResolvedTable, ResolvedValue,
    ScalarValue, ValueSourceKind, ValueSourceTrace, VectorValue,
};
pub use self::resolver::DataResolver;

#[cfg(test)]
pub use self::resolved::ResolvedDataSet;

#[cfg(test)]
mod tests;

pub(crate) fn data_diagnostic_error(
    code: &'static str,
    source: impl Into<String>,
    message: impl Into<String>,
    current: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            message,
            DiagnosticLocation::default(),
        )
        .with_source(source)
        .with_domain("data")
        .with_current_value(current)
        .with_expected(expected)
        .with_hint(hint),
    )
}
