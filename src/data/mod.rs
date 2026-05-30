// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod csv;
mod loader;
mod model;
mod plan;
mod resolved;
mod resolver;
pub(crate) mod value_conversion;

pub use self::csv::CsvDataReader;
pub use self::loader::DataLoader;
pub use self::model::{DataRow, DataSet, DataTable, RowSourceInfo, SourceRole};
pub use self::plan::{DataLoadPlan, DataProvider, DataSourceLayout, DataSourcePlan};
pub use self::resolved::{DataValue, ResolvedRow, ResolvedTable, ResolvedValue, ScalarValue};
pub use self::resolver::DataResolver;

#[cfg(test)]
pub use self::resolved::{ResolvedDataSet, ValueSourceKind};

#[cfg(test)]
mod tests;
