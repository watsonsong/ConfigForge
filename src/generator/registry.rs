// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use std::collections::BTreeMap;

use crate::{
    ConfigForgeError, Diagnostic, DiagnosticLocation, DiagnosticSeverity, Result,
    diagnostic::code::{CF_GENERATOR_AMBIGUOUS_TYPE, CF_GENERATOR_UNKNOWN_TYPE},
};

use super::{
    ArtifactGenerator, GeneratorRequirements, asset_manifest::AssetManifestExporter,
    binary::BinaryDataExporter, cpp::CppCodeGenerator, cpp_unreal::CppUnrealCodeGenerator,
    csharp::CSharpCodeGenerator, gettext_pot::GettextPotExporter, java::JavaCodeGenerator,
    json::JsonDataExporter, lua::LuaTableExporter, messagepack::MessagePackDataExporter,
    rust::RustCodeGenerator, schema_doc::SchemaDocGenerator, typescript::TypeScriptCodeGenerator,
    validation_report::ValidationReportGenerator,
};

pub(crate) struct GeneratorRegistry {
    generators: Vec<Box<dyn ArtifactGenerator>>,
}

impl GeneratorRegistry {
    pub(crate) fn new(generators: Vec<Box<dyn ArtifactGenerator>>) -> Self {
        Self { generators }
    }

    pub(crate) fn with_builtin_generators() -> Self {
        Self::new(vec![
            Box::new(RustCodeGenerator),
            Box::new(CppCodeGenerator),
            Box::new(CppUnrealCodeGenerator),
            Box::new(CSharpCodeGenerator),
            Box::new(JavaCodeGenerator),
            Box::new(TypeScriptCodeGenerator),
            Box::new(JsonDataExporter),
            Box::new(MessagePackDataExporter),
            Box::new(BinaryDataExporter),
            Box::new(GettextPotExporter),
            Box::new(LuaTableExporter),
            Box::new(AssetManifestExporter),
            Box::new(SchemaDocGenerator),
            Box::new(ValidationReportGenerator),
        ])
    }

    fn matching_generators(&self, type_id: &str) -> Vec<&dyn ArtifactGenerator> {
        self.generators
            .iter()
            .map(Box::as_ref)
            .filter(|generator| generator.descriptor().matches(type_id))
            .collect()
    }

    pub(crate) fn require_generator(&self, type_id: &str) -> Result<&dyn ArtifactGenerator> {
        let mut generators = self.matching_generators(type_id).into_iter();
        let Some(generator) = generators.next() else {
            return Err(unknown_generator_error(type_id, None));
        };
        if generators.next().is_some() {
            return Err(ambiguous_generator_error(type_id));
        }
        Ok(generator)
    }

    #[cfg(test)]
    pub(crate) fn validate_health(&self) -> Result<()> {
        let mut owners = BTreeMap::<&'static str, &'static str>::new();
        for generator in &self.generators {
            let descriptor = generator.descriptor();
            for name in
                std::iter::once(descriptor.type_id).chain(descriptor.aliases.iter().copied())
            {
                if let Some(previous) = owners.insert(name, descriptor.type_id) {
                    return Err(ConfigForgeError::from_diagnostic(
                        Diagnostic::new(
                            CF_GENERATOR_AMBIGUOUS_TYPE,
                            DiagnosticSeverity::Error,
                            format!(
                                "artifact generator name `{name}` is registered by both `{previous}` and `{}`",
                                descriptor.type_id
                            ),
                            DiagnosticLocation::default(),
                        )
                        .with_source("generator.registry")
                        .with_current_value(name)
                        .with_expected("globally unique generator type_id and alias"),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn requirements(
        &self,
        target_name: &str,
        type_id: &str,
    ) -> Result<GeneratorRequirements> {
        let mut generators = self.matching_generators(type_id).into_iter();
        let Some(generator) = generators.next() else {
            return Err(unknown_generator_error(type_id, Some(target_name)));
        };
        if generators.next().is_some() {
            return Err(ambiguous_generator_error(type_id));
        }
        Ok(generator.requirements())
    }
}

fn unknown_generator_error(type_id: &str, target_name: Option<&str>) -> ConfigForgeError {
    let message = match target_name {
        Some(target_name) => {
            format!("target `{target_name}` declares unknown export type `{type_id}`")
        }
        None => format!("no artifact generator registered for `{type_id}`"),
    };
    let mut diagnostic = Diagnostic::new(
        CF_GENERATOR_UNKNOWN_TYPE,
        DiagnosticSeverity::Error,
        message,
        DiagnosticLocation::default(),
    )
    .with_source("generator.registry")
    .with_current_value(type_id)
    .with_expected("registered generator type_id or alias");
    if let Some(target_name) = target_name {
        diagnostic = diagnostic.with_target(target_name);
    }
    ConfigForgeError::from_diagnostic(diagnostic)
}

fn ambiguous_generator_error(type_id: &str) -> ConfigForgeError {
    ConfigForgeError::from_diagnostic(
        Diagnostic::new(
            CF_GENERATOR_AMBIGUOUS_TYPE,
            DiagnosticSeverity::Error,
            format!("ambiguous artifact generator registration for `{type_id}`"),
            DiagnosticLocation::default(),
        )
        .with_source("generator.registry")
        .with_current_value(type_id)
        .with_expected("exactly one matching generator type_id or alias"),
    )
}

#[cfg(test)]
mod tests {
    use crate::generator::{GeneratorDescriptor, GeneratorRequirements};

    use super::*;

    struct StubGenerator {
        descriptor: GeneratorDescriptor,
    }

    #[test]
    fn every_builtin_generator_writes_exactly_its_planned_artifacts() {
        use crate::{
            ArtifactProjectInfo,
            artifact::{ArtifactWriteMode, ArtifactWriter},
            data::{DataSet, DataTable},
            generator::{GeneratorCommonInput, GeneratorTaskInput, PreparedGeneratorInputs},
            meta::{MetaModel, TargetTags},
        };
        let meta = MetaModel::from_toml_str(
            r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
        )
        .expect("meta");
        let linked = meta.link().expect("linked meta");
        let data_set = DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        )]);
        let report = crate::validation::ValidationReport::default();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool");
        let registry = GeneratorRegistry::with_builtin_generators();
        for generator in &registry.generators {
            for custom in [false, true] {
                let type_id = generator.type_id();
                let mut options = BTreeMap::new();
                if custom {
                    if matches!(type_id, "rust" | "cpp") {
                        options.insert(
                            type_id.to_owned(),
                            toml::Value::Table(toml::map::Map::from_iter([
                                (
                                    "runtime_interface".to_owned(),
                                    toml::Value::String("external".to_owned()),
                                ),
                                (
                                    "runtime_interface_emit".to_owned(),
                                    toml::Value::String("reference".to_owned()),
                                ),
                            ])),
                        );
                    }
                    if matches!(
                        type_id,
                        "rust" | "cpp" | "cpp_unreal" | "java" | "csharp" | "typescript"
                    ) {
                        options.insert(
                            "file_name".to_owned(),
                            toml::Value::String("CustomConfig".to_owned()),
                        );
                    }
                }
                let input = GeneratorCommonInput {
                    meta: &linked,
                    target_tags: TargetTags::empty(),
                    project: ArtifactProjectInfo::default(),
                    project_root: ".".into(),
                    target_name: "test".to_owned(),
                    task_name: type_id.to_owned(),
                    type_id: type_id.to_owned(),
                    output: "out".into(),
                    options,
                    generated_at: "2026-10-28T00:00:00Z".to_owned(),
                };
                let mut planned = generator.plan_artifacts(&input).expect(type_id);
                assert!(!planned.is_empty(), "{type_id} must declare its artifacts");
                let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();
                sink.reserve_paths(planned.clone()).expect(type_id);
                let task_input = GeneratorTaskInput {
                    common: input,
                    prepared: if generator.requirements().needs_data {
                        PreparedGeneratorInputs::MetaAndData {
                            data_set: &data_set,
                            validation_report: &report,
                            pool: &pool,
                        }
                    } else {
                        PreparedGeneratorInputs::MetaOnly
                    },
                };
                generator.generate(&task_input, &sink).expect(type_id);
                let actual = sink
                    .finish()
                    .expect(type_id)
                    .entries
                    .into_iter()
                    .map(|entry| entry.path)
                    .collect::<Vec<_>>();
                planned.sort();
                assert_eq!(planned, actual, "{type_id}, custom={custom}");
                if generator.requirements().needs_data {
                    let input = GeneratorTaskInput {
                        common: task_input.common,
                        prepared: PreparedGeneratorInputs::ValidationReportOnly {
                            validation_report: &report,
                        },
                    };
                    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();
                    sink.reserve_paths(planned.clone()).unwrap();
                    if generator.requirements().supports_report_only {
                        generator.generate(&input, &sink).unwrap();
                        assert_eq!(sink.finish().unwrap().entries.len(), planned.len());
                    } else {
                        let error = generator.generate(&input, &sink).unwrap_err();
                        let ConfigForgeError::ValidationReport(report) = error else {
                            panic!("expected structured unsupported input diagnostic");
                        };
                        assert_eq!(
                            report.diagnostics[0].code,
                            crate::diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT
                        );
                    }
                }
            }
        }
    }

    impl ArtifactGenerator for StubGenerator {
        fn plan_artifacts(
            &self,
            _input: &crate::generator::GeneratorCommonInput<'_>,
        ) -> Result<Vec<std::path::PathBuf>> {
            Ok(Vec::new())
        }

        fn type_id(&self) -> &'static str {
            self.descriptor.type_id
        }

        fn aliases(&self) -> &'static [&'static str] {
            self.descriptor.aliases
        }

        fn requirements(&self) -> GeneratorRequirements {
            self.descriptor.requirements
        }

        fn descriptor(&self) -> GeneratorDescriptor {
            self.descriptor
        }

        fn generate(
            &self,
            _input: &crate::generator::GeneratorTaskInput<'_, '_>,
            _sink: &crate::artifact::ArtifactSink,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn registry_reports_known_generator_requirements() {
        let registry = GeneratorRegistry::with_builtin_generators();
        registry
            .validate_health()
            .expect("healthy builtin registry");

        assert!(
            !registry
                .requirements("client", "rust")
                .expect("rust requirements")
                .needs_data
        );
        assert!(
            !registry
                .requirements("client", "ts")
                .expect("ts requirements")
                .needs_data
        );
        assert!(
            !registry
                .requirements("client", "schema_doc")
                .expect("schema_doc requirements")
                .needs_data
        );
        assert!(
            registry
                .requirements("client", "json")
                .expect("json requirements")
                .needs_data
        );
        assert!(
            registry
                .requirements("client", "msgpack")
                .expect("msgpack alias requirements")
                .needs_data
        );
        assert!(
            registry
                .requirements("client", "binary")
                .expect("binary alias requirements")
                .needs_data
        );
        assert!(
            registry
                .requirements("client", "lua")
                .expect("lua requirements")
                .needs_data
        );
        assert!(
            registry
                .requirements("client", "validation_report")
                .expect("validation_report requirements")
                .needs_data
        );
    }

    #[test]
    fn registry_reports_unknown_generator_type() {
        let registry = GeneratorRegistry::with_builtin_generators();

        let error = registry.requirements("client", "unknown").unwrap_err();
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured generator diagnostic");
        };
        assert_eq!(report.diagnostics[0].code, CF_GENERATOR_UNKNOWN_TYPE);
        assert_eq!(report.diagnostics[0].target.as_deref(), Some("client"));
        assert_eq!(
            report.diagnostics[0].current_value.as_deref(),
            Some("unknown")
        );
    }

    #[test]
    fn registry_reports_ambiguous_generator_registration() {
        let registry = GeneratorRegistry::new(vec![
            Box::new(StubGenerator {
                descriptor: GeneratorDescriptor::new(
                    "custom",
                    &[],
                    GeneratorRequirements::meta_only(),
                ),
            }),
            Box::new(StubGenerator {
                descriptor: GeneratorDescriptor::new(
                    "other",
                    &["custom"],
                    GeneratorRequirements::meta_only(),
                ),
            }),
        ]);

        let Err(error) = registry.require_generator("custom") else {
            panic!("expected ambiguous generator registration error");
        };
        let ConfigForgeError::ValidationReport(report) = error else {
            panic!("expected structured generator diagnostic");
        };
        assert_eq!(report.diagnostics[0].code, CF_GENERATOR_AMBIGUOUS_TYPE);
        assert_eq!(
            report.diagnostics[0].current_value.as_deref(),
            Some("custom")
        );
    }
}
