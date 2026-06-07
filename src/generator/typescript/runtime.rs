// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::code_artifact::CodeWriterLike;

pub(super) fn render_runtime_reader_interfaces(content: &mut impl CodeWriterLike) {
    content.raw(
        r#"export interface RuntimeDataReader {
  enterRequiredField(name: string): void;
  enterOptionalField(name: string): boolean;
  exitField(): void;
  enterIndex(index: number): void;
  exit(): void;
  readRowCount(): number;
  readListLength(): number;
  readEnumIndex(variants: readonly string[]): number;
  arrayElementsArePositional(): boolean;
  beginRecord(optionalSlotCount: number): void;
  endRecord(): void;
  asInt64(): number;
  asFloat64(): number;
  asBool(): boolean;
  asString(): string;
  schemaRevision?(): bigint | number | undefined;
  ensureExhausted?(): void;
}

export interface RuntimeDataLoader {
  open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader;
}

function isPromiseLike<T>(value: T | Promise<T>): value is Promise<T> {
  return typeof (value as { then?: unknown }).then === "function";
}

function openRuntimeDataSync(loader: RuntimeDataLoader, path: string): RuntimeDataReader {
  const reader = loader.open(path);
  if (isPromiseLike(reader)) {
    throw new Error(`runtime loader returned a Promise while synchronously opening ${path}`);
  }
  return reader;
}

"#,
    );
}

pub(super) fn render_runtime_reader_helpers(content: &mut impl CodeWriterLike) {
    content.raw(
        r#"function requiredField<T>(reader: RuntimeDataReader, name: string, convert: (value: RuntimeDataReader) => T): T {
  enterRequiredField(reader, name);
  try {
    return convert(reader);
  } finally {
    exitField(reader);
  }
}

function optionalField<T>(reader: RuntimeDataReader, name: string, convert: (value: RuntimeDataReader) => T): T | undefined {
  if (!enterOptionalField(reader, name)) {
    return undefined;
  }
  try {
    return convert(reader);
  } finally {
    exitField(reader);
  }
}

function enterRequiredField(reader: RuntimeDataReader, name: string): void {
  reader.enterRequiredField(name);
}

function enterOptionalField(reader: RuntimeDataReader, name: string): boolean {
  return reader.enterOptionalField(name);
}

function exitField(reader: RuntimeDataReader): void {
  reader.exitField();
}

function readEnumIndex(reader: RuntimeDataReader, variants: readonly string[]): number {
  return reader.readEnumIndex(variants);
}

function readRowCount(reader: RuntimeDataReader): number {
  return reader.readRowCount();
}

function readListLength(reader: RuntimeDataReader): number {
  return reader.readListLength();
}

function readRequiredInt64(reader: RuntimeDataReader, name: string): number {
  return requiredField(reader, name, (value) => value.asInt64());
}

function readRequiredFloat64(reader: RuntimeDataReader, name: string): number {
  return requiredField(reader, name, (value) => value.asFloat64());
}

function readRequiredBool(reader: RuntimeDataReader, name: string): boolean {
  return requiredField(reader, name, (value) => value.asBool());
}

function readRequiredString(reader: RuntimeDataReader, name: string): string {
  return requiredField(reader, name, (value) => value.asString());
}

function readOptionalInt64(reader: RuntimeDataReader, name: string): number | undefined {
  return optionalField(reader, name, (value) => value.asInt64());
}

function readOptionalFloat64(reader: RuntimeDataReader, name: string): number | undefined {
  return optionalField(reader, name, (value) => value.asFloat64());
}

function readOptionalBool(reader: RuntimeDataReader, name: string): boolean | undefined {
  return optionalField(reader, name, (value) => value.asBool());
}

function readOptionalString(reader: RuntimeDataReader, name: string): string | undefined {
  return optionalField(reader, name, (value) => value.asString());
}

function readList<T>(reader: RuntimeDataReader, convert: (value: RuntimeDataReader) => T): T[] {
  const length = readListLength(reader);
  const positional = reader.arrayElementsArePositional();
  const values: T[] = [];
  for (let index = 0; index < length; index += 1) {
    if (positional) {
      values.push(convert(reader));
      continue;
    }
    reader.enterIndex(index);
    try {
      values.push(convert(reader));
    } finally {
      reader.exit();
    }
  }
  return values;
}

function readRequiredList<T>(reader: RuntimeDataReader, name: string, convert: (value: RuntimeDataReader) => T): T[] {
  return requiredField(reader, name, (value) => readList(value, convert));
}

function readOptionalList<T>(reader: RuntimeDataReader, name: string, convert: (value: RuntimeDataReader) => T): T[] | undefined {
  return optionalField(reader, name, (value) => readList(value, convert));
}

"#,
    );
}
