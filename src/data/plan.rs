use std::path::PathBuf;

use crate::{
    meta::{MetaModel, SourceRef, TableMeta},
    ConfigForgeError, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataProvider {
    Csv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourcePlan {
    pub table_name: String,
    pub provider: DataProvider,
    pub path: PathBuf,
    pub fragments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLoadPlan {
    pub sources: Vec<DataSourcePlan>,
}

impl DataProvider {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Csv => "csv",
        }
    }
}

impl DataLoadPlan {
    pub fn from_meta_model(model: &MetaModel) -> Result<Self> {
        Self::from_tables(&model.tables)
    }

    pub fn from_tables<'a>(tables: impl IntoIterator<Item = &'a TableMeta>) -> Result<Self> {
        let mut sources = Vec::new();
        for table in tables {
            sources.extend(DataSourcePlan::from_table(table)?);
        }
        Ok(Self { sources })
    }

    pub fn sources(&self) -> &[DataSourcePlan] {
        &self.sources
    }
}

impl DataSourcePlan {
    pub fn from_table(table: &TableMeta) -> Result<Vec<Self>> {
        if table.source.is_some() && !table.sources.is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` declares both legacy `source` and `sources`",
                table.name
            )));
        }

        if table.sources.len() > 1 && !table.is_variant_source_mode() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` declares multiple sources, but only polymorphic collection tables with fragments support variant source mode",
                table.name
            )));
        }

        if !table.override_sources.is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` declares override_sources, but source override is not implemented yet",
                table.name
            )));
        }

        if !table.sources.is_empty() {
            return table
                .sources
                .iter()
                .map(|source| Self::from_source_ref(&table.name, source))
                .collect();
        }

        let path = if let Some(path) = &table.source {
            path
        } else {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` does not declare a source",
                table.name
            )));
        };

        Ok(vec![Self {
            table_name: table.name.clone(),
            provider: DataProvider::Csv,
            path: PathBuf::from(path),
            fragments: Vec::new(),
        }])
    }

    fn from_source_ref(table_name: &str, source: &SourceRef) -> Result<Self> {
        let provider = DataProvider::parse(&source.provider).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "table `{table_name}` source `{}` uses unsupported provider `{}`; only `csv` is supported",
                source.path, source.provider
            ))
        })?;

        Ok(Self {
            table_name: table_name.to_owned(),
            provider,
            path: PathBuf::from(&source.path),
            fragments: source.fragments.clone(),
        })
    }
}
