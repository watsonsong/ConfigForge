// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

mod fields;
mod model;
mod resolver;

#[cfg(test)]
mod tests;

pub(crate) use fields::asset_field_paths;
pub use model::{EffectiveAssetKind, ResolvedAssetRef};
pub use resolver::AssetResolver;
