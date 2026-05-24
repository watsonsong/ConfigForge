// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod csv;
mod loader;
mod model;
mod plan;
mod resolved;
mod resolver;

pub use self::csv::CsvDataReader;
pub use self::loader::DataLoader;
pub use self::model::{DataRow, DataSet, DataTable, RowSourceInfo};
pub use self::plan::{DataLoadPlan, DataProvider, DataSourceLayout, DataSourcePlan};
pub use self::resolved::{
    DataValue, ResolvedDataSet, ResolvedRow, ResolvedTable, ResolvedValue, ScalarValue,
    ValueSourceKind, ValueSourceTrace,
};
pub use self::resolver::{DataResolver, StructOccurrenceLayout, StructSourceLayout};

#[cfg(test)]
mod tests;
