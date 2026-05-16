use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MetaDiagnostic {
    pub code: MetaDiagnosticCode,
    pub message: String,
}

impl MetaDiagnostic {
    pub fn new(code: MetaDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for MetaDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MetaDiagnosticCode {
    DuplicateTableName,
    DuplicateEnumName,
    DuplicateStructName,
    DuplicateEnumValueName,
    DuplicateFieldName,
    DuplicateIndexName,
    MissingFieldReference,
    InvalidPrimaryIndex,
    PrimaryIndexMismatch,
    EmptyIndexFields,
    UnknownFieldType,
    InvalidDefaultValue,
    InvalidSourceDeclaration,
    UnsupportedFeature,
}
