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
    ArrayElementValue, DataValue, ResolvedRow, ResolvedTable, ResolvedValue, ScalarValue,
    ValueSourceKind,
};
pub use self::resolver::DataResolver;

#[cfg(test)]
pub use self::resolved::ResolvedDataSet;

#[cfg(test)]
mod tests;
