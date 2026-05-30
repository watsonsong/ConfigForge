// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::codegen::CodeWriter;

pub(super) fn render_runtime_reader_interfaces(content: &mut CodeWriter) {
    content.raw(
        r#"export interface RuntimeDataReader {
  enterField(name: string): boolean;
  enterIndex(index: number): void;
  exit(): void;
  length(): number;
  asNumber(): number;
  asBool(): boolean;
  asString(): string;
  schemaRevision?(): bigint | number | undefined;
  enterRequiredField?(name: string): void;
  enterOptionalField?(name: string): boolean;
  exitField?(): void;
  readEnumIndex?(variants: readonly string[]): number;
  readRowCount?(): number;
  readArrayLength?(): number;
  arrayElementsArePositional?(): boolean;
  beginRecord?(optionalSlotCount: number): void;
  endRecord?(): void;
  readOptionalPresent?(): boolean;
  readNextInt64?(): number;
  readNextFloat64?(): number;
  readNextNumber?(): number;
  readNextBool?(): boolean;
  readNextString?(): string;
  readEnumOrdinal?(): number;
  ensureExhausted?(): void;
}

export interface RuntimeDataLoader {
  open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader;
}

"#,
    );
}

pub(super) fn render_runtime_reader_helpers(content: &mut CodeWriter) {
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
  if (reader.enterRequiredField) {
    reader.enterRequiredField(name);
    return;
  }
  if (!reader.enterField(name)) {
    throw new Error(`missing required field ${name}`);
  }
}

function enterOptionalField(reader: RuntimeDataReader, name: string): boolean {
  if (reader.enterOptionalField) {
    return reader.enterOptionalField(name);
  }
  if (!reader.enterField(name)) {
    return false;
  }
  return true;
}

function exitField(reader: RuntimeDataReader): void {
  if (reader.exitField) {
    reader.exitField();
  } else {
    reader.exit();
  }
}

function readEnumIndex(reader: RuntimeDataReader, variants: readonly string[]): number {
  if (reader.readEnumIndex) {
    return reader.readEnumIndex(variants);
  }
  const value = reader.asString();
  const index = variants.indexOf(value);
  if (index < 0) {
    throw new Error(`unknown enum value ${value}`);
  }
  return index;
}

function readRowCount(reader: RuntimeDataReader): number {
  return reader.readRowCount ? reader.readRowCount() : reader.length();
}

function readArrayLength(reader: RuntimeDataReader): number {
  return reader.readArrayLength ? reader.readArrayLength() : reader.length();
}

function readRequiredInt64(reader: RuntimeDataReader, name: string): number {
  return requiredField(reader, name, (value) => value.readNextInt64 ? value.readNextInt64() : value.asNumber());
}

function readRequiredFloat64(reader: RuntimeDataReader, name: string): number {
  return requiredField(reader, name, (value) => value.readNextFloat64 ? value.readNextFloat64() : value.asNumber());
}

function readRequiredBool(reader: RuntimeDataReader, name: string): boolean {
  return requiredField(reader, name, (value) => value.asBool());
}

function readRequiredString(reader: RuntimeDataReader, name: string): string {
  return requiredField(reader, name, (value) => value.asString());
}

function readOptionalInt64(reader: RuntimeDataReader, name: string): number | undefined {
  return optionalField(reader, name, (value) => value.readNextInt64 ? value.readNextInt64() : value.asNumber());
}

function readOptionalFloat64(reader: RuntimeDataReader, name: string): number | undefined {
  return optionalField(reader, name, (value) => value.readNextFloat64 ? value.readNextFloat64() : value.asNumber());
}

function readOptionalBool(reader: RuntimeDataReader, name: string): boolean | undefined {
  return optionalField(reader, name, (value) => value.asBool());
}

function readOptionalString(reader: RuntimeDataReader, name: string): string | undefined {
  return optionalField(reader, name, (value) => value.asString());
}

function readList<T>(reader: RuntimeDataReader, convert: (value: RuntimeDataReader) => T): T[] {
  const length = readArrayLength(reader);
  const values: T[] = [];
  for (let index = 0; index < length; index += 1) {
    if (reader.arrayElementsArePositional && reader.arrayElementsArePositional()) {
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
