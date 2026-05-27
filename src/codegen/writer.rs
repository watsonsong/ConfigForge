// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

/// Shared configuration for generated source formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeWriterConfig {
    pub indent_style: IndentStyle,
    pub indent_width: usize,
    pub trailing_newline: bool,
    pub trailing_blank_line: bool,
}

impl CodeWriterConfig {
    pub fn with_indent_width(mut self, indent_width: usize) -> Self {
        self.indent_width = indent_width;
        self
    }

    pub fn merge(self, partial: PartialCodeWriterConfig) -> Self {
        Self {
            indent_style: partial.indent_style.unwrap_or(self.indent_style),
            indent_width: partial.indent_width.unwrap_or(self.indent_width),
            trailing_newline: partial.trailing_newline.unwrap_or(self.trailing_newline),
            trailing_blank_line: partial
                .trailing_blank_line
                .unwrap_or(self.trailing_blank_line),
        }
    }
}

impl Default for CodeWriterConfig {
    fn default() -> Self {
        Self {
            indent_style: IndentStyle::Spaces,
            indent_width: 4,
            trailing_newline: true,
            trailing_blank_line: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndentStyle {
    Spaces,
    Tab,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub struct PartialCodeWriterConfig {
    #[serde(default)]
    pub indent_style: Option<IndentStyle>,
    #[serde(default)]
    pub indent_width: Option<usize>,
    #[serde(default)]
    pub trailing_newline: Option<bool>,
    #[serde(default)]
    pub trailing_blank_line: Option<bool>,
}

/// Streaming helper for deterministic source generation.
#[derive(Debug, Clone)]
pub struct CodeWriter {
    output: String,
    config: CodeWriterConfig,
    indent_level: usize,
    line_start: bool,
}

impl CodeWriter {
    pub fn new(config: CodeWriterConfig) -> Self {
        Self {
            output: String::new(),
            config,
            indent_level: 0,
            line_start: true,
        }
    }

    pub fn with_content(content: impl Into<String>, config: CodeWriterConfig) -> Self {
        let output = content.into();
        let line_start = output.ends_with('\n') || output.is_empty();
        Self {
            output,
            config,
            indent_level: 0,
            line_start,
        }
    }

    pub fn config(&self) -> CodeWriterConfig {
        self.config
    }

    pub fn indent_level(&self) -> usize {
        self.indent_level
    }

    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn dedent(&mut self) {
        self.indent_level = self
            .indent_level
            .checked_sub(1)
            .expect("CodeWriter dedent called at indentation level 0");
    }

    pub fn indented<R>(&mut self, write_body: impl FnOnce(&mut Self) -> R) -> R {
        self.indent();
        let result = write_body(self);
        self.dedent();
        result
    }

    pub fn raw(&mut self, text: impl AsRef<str>) {
        self.output.push_str(text.as_ref());
        self.line_start = self.output.ends_with('\n') || self.output.is_empty();
    }

    pub fn write(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        for segment in text.split_inclusive('\n') {
            if self.line_start && segment != "\n" {
                self.write_indent();
            }
            self.output.push_str(segment);
            self.line_start = segment.ends_with('\n');
        }
    }

    pub fn line(&mut self, text: impl AsRef<str>) {
        self.write(text);
        self.newline();
    }

    pub fn blank_line(&mut self) {
        self.newline();
    }

    pub fn newline(&mut self) {
        self.output.push('\n');
        self.line_start = true;
    }

    pub fn finish(mut self) -> String {
        if self.config.trailing_blank_line {
            self.ensure_trailing_blank_line();
        } else if self.config.trailing_newline {
            self.ensure_trailing_newline();
        } else {
            while self.output.ends_with('\n') {
                self.output.pop();
            }
        }
        self.output
    }

    fn write_indent(&mut self) {
        match self.config.indent_style {
            IndentStyle::Spaces => {
                for _ in 0..self.indent_level * self.config.indent_width {
                    self.output.push(' ');
                }
            }
            IndentStyle::Tab => {
                for _ in 0..self.indent_level {
                    self.output.push('\t');
                }
            }
        }
        self.line_start = false;
    }

    fn ensure_trailing_newline(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn ensure_trailing_blank_line(&mut self) {
        if self.output.is_empty() {
            return;
        }
        self.ensure_trailing_newline();
        if !self.output.ends_with("\n\n") {
            self.output.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_indented_lines_with_spaces() {
        let mut writer = CodeWriter::new(CodeWriterConfig::default().with_indent_width(2));

        writer.line("export interface Item {");
        writer.indented(|writer| {
            writer.line("id: number;");
            writer.line("name: string;");
        });
        writer.line("}");

        assert_eq!(
            writer.finish(),
            "export interface Item {\n  id: number;\n  name: string;\n}\n"
        );
    }

    #[test]
    fn writes_indented_lines_with_tabs() {
        let mut writer = CodeWriter::new(CodeWriterConfig {
            indent_style: IndentStyle::Tab,
            ..CodeWriterConfig::default()
        });

        writer.line("class Reader {");
        writer.indented(|writer| writer.line("public:"));
        writer.line("};");

        assert_eq!(writer.finish(), "class Reader {\n\tpublic:\n};\n");
    }

    #[test]
    fn raw_preserves_existing_text_without_auto_indent() {
        let mut writer = CodeWriter::new(CodeWriterConfig::default());

        writer.indent();
        writer.raw("already formatted\n");
        writer.line("auto formatted");

        assert_eq!(writer.finish(), "already formatted\n    auto formatted\n");
    }

    #[test]
    fn finish_can_omit_trailing_newline() {
        let mut writer = CodeWriter::new(CodeWriterConfig {
            trailing_newline: false,
            ..CodeWriterConfig::default()
        });

        writer.line("let value = 1;");

        assert_eq!(writer.finish(), "let value = 1;");
    }

    #[test]
    fn finish_can_ensure_trailing_blank_line() {
        let mut writer = CodeWriter::new(CodeWriterConfig {
            trailing_blank_line: true,
            ..CodeWriterConfig::default()
        });

        writer.line("let value = 1;");

        assert_eq!(writer.finish(), "let value = 1;\n\n");
    }

    #[test]
    fn partial_config_overrides_defaults() {
        let config = CodeWriterConfig::default().merge(PartialCodeWriterConfig {
            indent_style: Some(IndentStyle::Tab),
            indent_width: Some(8),
            trailing_newline: Some(false),
            trailing_blank_line: Some(true),
        });

        assert_eq!(config.indent_style, IndentStyle::Tab);
        assert_eq!(config.indent_width, 8);
        assert!(!config.trailing_newline);
        assert!(config.trailing_blank_line);
    }
}
