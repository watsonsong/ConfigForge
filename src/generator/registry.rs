// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ConfigForgeError, Result};

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
            return Err(ConfigForgeError::Validation(format!(
                "no artifact generator registered for `{type_id}`"
            )));
        };
        if generators.next().is_some() {
            return Err(ConfigForgeError::Validation(format!(
                "ambiguous artifact generator registration for `{type_id}`"
            )));
        }
        Ok(generator)
    }

    pub(crate) fn requirements(
        &self,
        target_name: &str,
        type_id: &str,
    ) -> Result<GeneratorRequirements> {
        match self.require_generator(type_id) {
            Ok(generator) => Ok(generator.requirements()),
            Err(ConfigForgeError::Validation(message))
                if message.contains("no artifact generator registered") =>
            {
                Err(ConfigForgeError::Validation(format!(
                    "target `{target_name}` declares unknown export type `{type_id}`"
                )))
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::generator::{GeneratorDescriptor, GeneratorRequirements};

    use super::*;

    struct StubGenerator {
        descriptor: GeneratorDescriptor,
    }

    impl ArtifactGenerator for StubGenerator {
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

        fn generate_to_sink(
            &self,
            _input: &crate::generator::GeneratorCommonInput<'_>,
            _sink: &crate::artifact::ArtifactSink,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn registry_reports_known_generator_requirements() {
        let registry = GeneratorRegistry::with_builtin_generators();

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

        assert!(
            matches!(registry.requirements("client", "unknown"), Err(ConfigForgeError::Validation(message)) if message.contains("unknown export type `unknown`"))
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

        assert!(
            matches!(registry.require_generator("custom"), Err(ConfigForgeError::Validation(message)) if message.contains("ambiguous artifact generator registration for `custom`"))
        );
    }
}
