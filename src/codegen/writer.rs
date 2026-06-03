// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use std::{
    fmt::{self, Write as _},
    io::{self, Write},
};

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
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct CodeWriter {
    output: String,
    config: CodeWriterConfig,
    indent_level: usize,
    line_start: bool,
}

#[cfg(test)]
#[allow(dead_code)]
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

    pub fn raw_fmt(&mut self, args: fmt::Arguments<'_>) {
        self.output
            .write_fmt(args)
            .expect("writing to String should not fail");
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

    pub fn line_fmt(&mut self, args: fmt::Arguments<'_>) {
        if self.line_start {
            self.write_indent();
        }
        self.output
            .write_fmt(args)
            .expect("writing to String should not fail");
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

/// Streaming source writer with the same formatting rules as [`CodeWriter`].
///
/// The writer forwards completed bytes to the wrapped `std::io::Write` immediately, keeping only a
/// small count of trailing newline bytes so `finish()` can apply the same trailing newline / blank
/// line policy as the string-backed writer without retaining the whole generated file in memory.
#[derive(Debug)]
pub struct StreamingCodeWriter<W> {
    output: W,
    config: CodeWriterConfig,
    indent_level: usize,
    line_start: bool,
    pending_newlines: usize,
    has_content: bool,
}

pub trait CodeWriterLike {
    fn raw(&mut self, text: impl AsRef<str>);
    fn raw_fmt(&mut self, args: fmt::Arguments<'_>);
    fn write(&mut self, text: impl AsRef<str>);
    fn line(&mut self, text: impl AsRef<str>);
    fn line_fmt(&mut self, args: fmt::Arguments<'_>);
    fn blank_line(&mut self);
    fn newline(&mut self);
}

#[cfg(test)]
impl CodeWriterLike for CodeWriter {
    fn raw(&mut self, text: impl AsRef<str>) {
        self.raw(text);
    }

    fn raw_fmt(&mut self, args: fmt::Arguments<'_>) {
        self.raw_fmt(args);
    }

    fn write(&mut self, text: impl AsRef<str>) {
        self.write(text);
    }

    fn line(&mut self, text: impl AsRef<str>) {
        self.line(text);
    }

    fn line_fmt(&mut self, args: fmt::Arguments<'_>) {
        self.line_fmt(args);
    }

    fn blank_line(&mut self) {
        self.blank_line();
    }

    fn newline(&mut self) {
        self.newline();
    }
}

impl<T: CodeWriterLike + ?Sized> CodeWriterLike for &mut T {
    fn raw(&mut self, text: impl AsRef<str>) {
        (**self).raw(text);
    }

    fn raw_fmt(&mut self, args: fmt::Arguments<'_>) {
        (**self).raw_fmt(args);
    }

    fn write(&mut self, text: impl AsRef<str>) {
        (**self).write(text);
    }

    fn line(&mut self, text: impl AsRef<str>) {
        (**self).line(text);
    }

    fn line_fmt(&mut self, args: fmt::Arguments<'_>) {
        (**self).line_fmt(args);
    }

    fn blank_line(&mut self) {
        (**self).blank_line();
    }

    fn newline(&mut self) {
        (**self).newline();
    }
}

#[derive(Debug)]
pub struct CheckedStreamingCodeWriter<W> {
    inner: StreamingCodeWriter<W>,
    error: Option<io::Error>,
}

impl<W: Write> CheckedStreamingCodeWriter<W> {
    pub fn new(output: W, config: CodeWriterConfig) -> Self {
        Self {
            inner: StreamingCodeWriter::new(output, config),
            error: None,
        }
    }

    pub fn finish(self) -> io::Result<W> {
        if let Some(error) = self.error {
            return Err(error);
        }
        self.inner.finish()
    }

    fn record(&mut self, result: io::Result<()>) {
        if let Err(error) = result
            && self.error.is_none()
        {
            self.error = Some(error);
        }
    }
}

impl<W: Write> CodeWriterLike for CheckedStreamingCodeWriter<W> {
    fn raw(&mut self, text: impl AsRef<str>) {
        if self.error.is_none() {
            let result = self.inner.raw(text);
            self.record(result);
        }
    }

    fn raw_fmt(&mut self, args: fmt::Arguments<'_>) {
        if self.error.is_none() {
            let result = self.inner.raw_fmt(args);
            self.record(result);
        }
    }

    fn write(&mut self, text: impl AsRef<str>) {
        if self.error.is_none() {
            let result = self.inner.write(text);
            self.record(result);
        }
    }

    fn line(&mut self, text: impl AsRef<str>) {
        if self.error.is_none() {
            let result = self.inner.line(text);
            self.record(result);
        }
    }

    fn line_fmt(&mut self, args: fmt::Arguments<'_>) {
        if self.error.is_none() {
            let result = self.inner.line_fmt(args);
            self.record(result);
        }
    }

    fn blank_line(&mut self) {
        if self.error.is_none() {
            let result = self.inner.blank_line();
            self.record(result);
        }
    }

    fn newline(&mut self) {
        if self.error.is_none() {
            let result = self.inner.newline();
            self.record(result);
        }
    }
}

impl<W: Write> StreamingCodeWriter<W> {
    pub fn new(output: W, config: CodeWriterConfig) -> Self {
        Self {
            output,
            config,
            indent_level: 0,
            line_start: true,
            pending_newlines: 0,
            has_content: false,
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
            .expect("StreamingCodeWriter dedent called at indentation level 0");
    }

    pub fn indented<R>(&mut self, write_body: impl FnOnce(&mut Self) -> R) -> R {
        self.indent();
        let result = write_body(self);
        self.dedent();
        result
    }

    pub fn raw(&mut self, text: impl AsRef<str>) -> io::Result<()> {
        let text = text.as_ref();
        if text.is_empty() {
            return Ok(());
        }

        let trailing_newlines = text
            .as_bytes()
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\n')
            .count();
        let body_len = text.len() - trailing_newlines;
        if body_len > 0 {
            self.flush_pending_newlines()?;
            self.output.write_all(&text.as_bytes()[..body_len])?;
            self.has_content = true;
            self.line_start = false;
        }
        if trailing_newlines > 0 {
            self.pending_newlines = self.pending_newlines.saturating_add(trailing_newlines);
            self.has_content = true;
            self.line_start = true;
        }
        Ok(())
    }

    pub fn raw_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
        let mut text = String::new();
        text.write_fmt(args)
            .expect("writing fmt::Arguments to String should not fail");
        self.raw(text)
    }

    pub fn write(&mut self, text: impl AsRef<str>) -> io::Result<()> {
        for segment in text.as_ref().split_inclusive('\n') {
            if self.line_start && segment != "\n" {
                self.write_indent()?;
            }
            self.raw(segment)?;
        }
        Ok(())
    }

    pub fn line(&mut self, text: impl AsRef<str>) -> io::Result<()> {
        self.write(text)?;
        self.newline()
    }

    pub fn line_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
        if self.line_start {
            self.write_indent()?;
        }
        self.raw_fmt(args)?;
        self.newline()
    }

    pub fn blank_line(&mut self) -> io::Result<()> {
        self.newline()
    }

    pub fn newline(&mut self) -> io::Result<()> {
        self.pending_newlines = self.pending_newlines.saturating_add(1);
        self.has_content = true;
        self.line_start = true;
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<W> {
        if self.config.trailing_blank_line {
            self.ensure_trailing_blank_line();
        } else if self.config.trailing_newline {
            self.ensure_trailing_newline();
        } else {
            self.pending_newlines = 0;
        }
        self.flush_pending_newlines()?;
        self.output.flush()?;
        Ok(self.output)
    }

    fn write_indent(&mut self) -> io::Result<()> {
        self.flush_pending_newlines()?;
        match self.config.indent_style {
            IndentStyle::Spaces => {
                for _ in 0..self.indent_level * self.config.indent_width {
                    self.output.write_all(b" ")?;
                    self.has_content = true;
                }
            }
            IndentStyle::Tab => {
                for _ in 0..self.indent_level {
                    self.output.write_all(b"\t")?;
                    self.has_content = true;
                }
            }
        }
        self.line_start = false;
        Ok(())
    }

    fn ensure_trailing_newline(&mut self) {
        if self.has_content && self.pending_newlines == 0 {
            self.pending_newlines = 1;
        }
    }

    fn ensure_trailing_blank_line(&mut self) {
        if self.has_content && self.pending_newlines < 2 {
            self.pending_newlines = 2;
        }
    }

    fn flush_pending_newlines(&mut self) -> io::Result<()> {
        for _ in 0..self.pending_newlines {
            self.output.write_all(b"\n")?;
        }
        self.pending_newlines = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io;

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

    #[test]
    fn streaming_writer_matches_string_writer_for_common_formatting() {
        let config = CodeWriterConfig::default().with_indent_width(2);
        let mut string_writer = CodeWriter::new(config);
        let mut stream_writer = StreamingCodeWriter::new(Vec::new(), config);

        string_writer.line("export interface Item {");
        stream_writer
            .line("export interface Item {")
            .expect("line should write");
        string_writer.indented(|writer| {
            writer.line("id: number;");
            writer.line_fmt(format_args!("name: {};", "string"));
            writer.blank_line();
            writer.write("readonly tags: string[];");
        });
        stream_writer
            .indented(|writer| -> io::Result<()> {
                writer.line("id: number;")?;
                writer.line_fmt(format_args!("name: {};", "string"))?;
                writer.blank_line()?;
                writer.write("readonly tags: string[];")?;
                Ok(())
            })
            .expect("indented body should write");
        string_writer.newline();
        stream_writer.newline().expect("newline should write");
        string_writer.line("}");
        stream_writer.line("}").expect("line should write");

        let stream_output = String::from_utf8(
            stream_writer
                .finish()
                .expect("streaming writer should finish"),
        )
        .expect("generated output should be utf8");

        assert_eq!(stream_output, string_writer.finish());
    }

    #[test]
    fn streaming_writer_matches_tab_indent_and_raw_behavior() {
        let config = CodeWriterConfig {
            indent_style: IndentStyle::Tab,
            ..CodeWriterConfig::default()
        };
        let mut string_writer = CodeWriter::new(config);
        let mut stream_writer = StreamingCodeWriter::new(Vec::new(), config);

        string_writer.indent();
        stream_writer.indent();
        string_writer.raw("already formatted\n");
        stream_writer
            .raw("already formatted\n")
            .expect("raw should write");
        string_writer.line("auto formatted");
        stream_writer
            .line("auto formatted")
            .expect("line should write");

        let stream_output = String::from_utf8(
            stream_writer
                .finish()
                .expect("streaming writer should finish"),
        )
        .expect("generated output should be utf8");

        assert_eq!(stream_output, string_writer.finish());
    }

    #[test]
    fn streaming_writer_can_omit_trailing_newline_without_buffering_full_output() {
        let config = CodeWriterConfig {
            trailing_newline: false,
            ..CodeWriterConfig::default()
        };
        let mut writer = StreamingCodeWriter::new(Vec::new(), config);

        writer.line("let value = 1;").expect("line should write");
        writer.blank_line().expect("blank line should write");

        let output = String::from_utf8(writer.finish().expect("writer should finish"))
            .expect("generated output should be utf8");
        assert_eq!(output, "let value = 1;");
    }

    #[test]
    fn streaming_writer_can_ensure_trailing_blank_line() {
        let config = CodeWriterConfig {
            trailing_blank_line: true,
            ..CodeWriterConfig::default()
        };
        let mut writer = StreamingCodeWriter::new(Vec::new(), config);

        writer.line("let value = 1;").expect("line should write");

        let output = String::from_utf8(writer.finish().expect("writer should finish"))
            .expect("generated output should be utf8");
        assert_eq!(output, "let value = 1;\n\n");
    }

    #[test]
    fn streaming_writer_propagates_write_errors() {
        #[derive(Debug)]
        struct FailingWriter;

        impl io::Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("forced failure"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut writer = StreamingCodeWriter::new(FailingWriter, CodeWriterConfig::default());

        let error = writer
            .line("will fail")
            .expect_err("writer should propagate io errors");

        assert_eq!(error.kind(), io::ErrorKind::Other);
    }
}
