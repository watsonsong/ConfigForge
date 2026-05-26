// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod runtime_reader;

pub use config_forge_runtime_reader_core::{
    CONFIG_FORGE_READER_CONTRACT_VERSION, RuntimeDataLoader, RuntimeDataReader,
    SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
};
pub use runtime_reader::{BinaryRuntimeDataReader, FsBinaryRuntimeDataLoader};
