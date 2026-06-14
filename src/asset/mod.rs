// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod model;
mod resolver;

#[cfg(test)]
mod tests;

pub use model::{EffectiveAssetKind, ResolvedAssetRef};
pub use resolver::AssetResolver;
