// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

/// Input requirements declared by a build-facing artifact generator.
///
/// Requirements describe which shared build inputs must be prepared before a
/// generator can run. They are scheduling metadata, not a trait inheritance
/// hierarchy: a data-backed generator may still produce code-like artifacts,
/// and a Meta-only generator may produce docs or manifests rather than code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GeneratorRequirements {
    pub(crate) needs_meta: bool,
    pub(crate) needs_data: bool,
    pub(crate) needs_assets: bool,
    pub(crate) needs_validation_report: bool,
}

impl GeneratorRequirements {
    pub(crate) const fn meta_only() -> Self {
        Self {
            needs_meta: true,
            needs_data: false,
            needs_assets: false,
            needs_validation_report: false,
        }
    }

    pub(crate) const fn meta_and_data() -> Self {
        Self {
            needs_meta: true,
            needs_data: true,
            needs_assets: false,
            needs_validation_report: false,
        }
    }

    pub(crate) const fn with_assets(mut self) -> Self {
        self.needs_assets = true;
        self
    }

    pub(crate) const fn with_validation_report(mut self) -> Self {
        self.needs_validation_report = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GeneratorDescriptor {
    pub(crate) type_id: &'static str,
    pub(crate) aliases: &'static [&'static str],
    pub(crate) requirements: GeneratorRequirements,
}

impl GeneratorDescriptor {
    pub(crate) const fn new(
        type_id: &'static str,
        aliases: &'static [&'static str],
        requirements: GeneratorRequirements,
    ) -> Self {
        Self {
            type_id,
            aliases,
            requirements,
        }
    }

    pub(crate) fn matches(&self, type_id: &str) -> bool {
        self.type_id == type_id || self.aliases.contains(&type_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_without_data_do_not_need_data() {
        let requirements = GeneratorRequirements::meta_only();

        assert!(requirements.needs_meta);
        assert!(!requirements.needs_data);
    }

    #[test]
    fn requirements_with_data_need_data() {
        let requirements = GeneratorRequirements::meta_and_data();

        assert!(requirements.needs_meta);
        assert!(requirements.needs_data);
    }

    #[test]
    fn auxiliary_requirements_do_not_force_data_dependency() {
        let meta_with_assets = GeneratorRequirements::meta_only().with_assets();
        let meta_with_report = GeneratorRequirements::meta_only().with_validation_report();

        assert!(!meta_with_assets.needs_data);
        assert!(meta_with_assets.needs_assets);
        assert!(!meta_with_report.needs_data);
        assert!(meta_with_report.needs_validation_report);
    }

    #[test]
    fn descriptor_matches_type_id_and_aliases() {
        let descriptor = GeneratorDescriptor::new(
            "messagepack",
            &["msgpack", "mpack"],
            GeneratorRequirements::meta_and_data(),
        );

        assert!(descriptor.matches("messagepack"));
        assert!(descriptor.matches("msgpack"));
        assert!(descriptor.matches("mpack"));
        assert!(!descriptor.matches("json"));
        assert!(descriptor.requirements.needs_data);
    }
}
