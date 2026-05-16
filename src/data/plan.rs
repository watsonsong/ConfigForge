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
            sources.push(DataSourcePlan::from_table(table)?);
        }
        Ok(Self { sources })
    }

    pub fn sources(&self) -> &[DataSourcePlan] {
        &self.sources
    }
}

impl DataSourcePlan {
    pub fn from_table(table: &TableMeta) -> Result<Self> {
        if table.source.is_some() && !table.sources.is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` declares both legacy `source` and `sources`",
                table.name
            )));
        }

        if table.sources.len() > 1 {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` declares multiple sources, but multi-source merge is not implemented yet",
                table.name
            )));
        }

        if !table.override_sources.is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` declares override_sources, but source override is not implemented yet",
                table.name
            )));
        }

        if let Some(source) = table.sources.first() {
            return Self::from_source_ref(&table.name, source);
        }

        let path = if let Some(path) = &table.source {
            path
        } else {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` does not declare a source",
                table.name
            )));
        };

        Ok(Self {
            table_name: table.name.clone(),
            provider: DataProvider::Csv,
            path: PathBuf::from(path),
        })
    }

    fn from_source_ref(table_name: &str, source: &SourceRef) -> Result<Self> {
        if !source.fragments.is_empty() {
            return Err(ConfigForgeError::Validation(format!(
                "table `{table_name}` source `{}` declares fragments, but polymorphic fragments are not implemented yet",
                source.path
            )));
        }

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
        })
    }
}
