// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, sync::mpsc, thread};

use crate::{ConfigForgeError, Result, diagnostic::code::CF_BUILD_INVALID_OPTION};

use super::data_diagnostic_error;

use super::{DataLoader, DataSourcePlan, DataTable, RawSourceData, SourceReadPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceProviderResource {
    #[default]
    BlockingLocal,
    AsyncIo,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceProviderConcurrency {
    pub blocking_local: usize,
    pub async_io: usize,
    pub network: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Reserved policy metadata; the compatibility shell does not apply retries or rate limits.
pub struct RemoteProviderPolicy {
    pub max_retries: usize,
    pub base_backoff_ms: u64,
    pub rate_limit_per_second: Option<usize>,
}

pub trait SourceProvider: Sync {
    fn resource(&self, source: &DataSourcePlan) -> SourceProviderResource;

    fn read_source(&self, source: &DataSourcePlan) -> Result<DataTable>;
}

#[derive(Debug, Clone)]
pub struct LocalCsvSourceProvider {
    loader: DataLoader,
}

#[derive(Debug, Clone)]
/// Compatibility resource-lane scheduler using blocking scoped threads.
/// The build pipeline uses its existing Rayon pool instead of this shell.
pub struct AsyncSourceReadShell {
    concurrency: SourceProviderConcurrency,
}

impl SourceProviderResource {
    fn index(self) -> usize {
        match self {
            Self::BlockingLocal => 0,
            Self::AsyncIo => 1,
            Self::Network => 2,
        }
    }
}

impl SourceProviderConcurrency {
    pub fn new(blocking_local: usize, async_io: usize, network: usize) -> Result<Self> {
        if blocking_local == 0 {
            return Err(source_concurrency_error(
                "blocking local source concurrency must be greater than 0".to_owned(),
                "blocking_local=0",
                "positive blocking local source concurrency",
            ));
        }
        if async_io == 0 {
            return Err(source_concurrency_error(
                "async source concurrency must be greater than 0".to_owned(),
                "async_io=0",
                "positive async source concurrency",
            ));
        }
        if network == 0 {
            return Err(source_concurrency_error(
                "network source concurrency must be greater than 0".to_owned(),
                "network=0",
                "positive network source concurrency",
            ));
        }
        Ok(Self {
            blocking_local,
            async_io,
            network,
        })
    }

    fn limit_for(&self, resource: SourceProviderResource) -> usize {
        match resource {
            SourceProviderResource::BlockingLocal => self.blocking_local,
            SourceProviderResource::AsyncIo => self.async_io,
            SourceProviderResource::Network => self.network,
        }
    }
}

fn source_concurrency_error(
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
) -> ConfigForgeError {
    data_diagnostic_error(
        CF_BUILD_INVALID_OPTION,
        "data.source_provider.concurrency",
        message,
        current,
        expected,
        "Set source provider concurrency limits to values greater than 0.",
    )
}

impl Default for SourceProviderConcurrency {
    fn default() -> Self {
        Self {
            blocking_local: 1,
            async_io: 4,
            network: 4,
        }
    }
}

impl Default for RemoteProviderPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_backoff_ms: 200,
            rate_limit_per_second: None,
        }
    }
}

impl LocalCsvSourceProvider {
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        Self {
            loader: DataLoader::new(data_root),
        }
    }
}

impl SourceProvider for LocalCsvSourceProvider {
    fn resource(&self, _source: &DataSourcePlan) -> SourceProviderResource {
        SourceProviderResource::BlockingLocal
    }

    fn read_source(&self, source: &DataSourcePlan) -> Result<DataTable> {
        self.loader.load_source(source)
    }
}

impl AsyncSourceReadShell {
    pub fn new(concurrency: SourceProviderConcurrency) -> Self {
        Self { concurrency }
    }

    pub fn read_sources(
        &self,
        plan: &SourceReadPlan,
        provider: &impl SourceProvider,
    ) -> Result<RawSourceData> {
        let sources = plan.sources();
        let mut results = (0..sources.len()).map(|_| None).collect::<Vec<_>>();
        let mut started = vec![false; sources.len()];
        let mut running_by_resource = [0_usize; 3];
        let mut in_flight = 0_usize;
        let mut stop_scheduling = false;
        let (tx, rx) = mpsc::channel();

        thread::scope(|scope| {
            loop {
                if !stop_scheduling {
                    for (index, source) in sources.iter().enumerate() {
                        if started[index] {
                            continue;
                        }
                        let resource = provider.resource(source);
                        let resource_index = resource.index();
                        if running_by_resource[resource_index]
                            >= self.concurrency.limit_for(resource)
                        {
                            continue;
                        }
                        started[index] = true;
                        running_by_resource[resource_index] += 1;
                        in_flight += 1;
                        let tx = tx.clone();
                        scope.spawn(move || {
                            let result = provider.read_source(source);
                            let _ = tx.send(SourceReadTaskResult {
                                index,
                                resource,
                                result,
                            });
                        });
                    }
                }

                if in_flight == 0 {
                    break;
                }

                let task_result = rx
                    .recv()
                    .expect("source read worker channel should stay open while tasks are running");
                in_flight -= 1;
                running_by_resource[task_result.resource.index()] -= 1;
                if task_result.result.is_err() {
                    stop_scheduling = true;
                }
                results[task_result.index] = Some(task_result.result);
            }
        });

        let mut loaded = Vec::new();
        let mut first_error = None;
        for (index, result) in results.into_iter().enumerate() {
            let Some(result) = result else {
                continue;
            };
            match result {
                Ok(table) => loaded.push((index, &sources[index], table)),
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }

        Ok(RawSourceData::new(super::loader::merge_loaded_sources(
            loaded,
        )?))
    }
}

impl Default for AsyncSourceReadShell {
    fn default() -> Self {
        Self::new(SourceProviderConcurrency::default())
    }
}

struct SourceReadTaskResult {
    index: usize,
    resource: SourceProviderResource,
    result: Result<DataTable>,
}
