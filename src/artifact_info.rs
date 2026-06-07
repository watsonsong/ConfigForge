// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtifactProjectInfo {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub copyright: Option<String>,
}
