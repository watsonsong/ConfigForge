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
  isNull(): boolean;
  schemaRevision?(): bigint | number | undefined;
  isPositional?(): boolean;
  readRowCount?(): number;
  beginRecord?(optionalSlotCount: number): void;
  endRecord?(): void;
  readOptionalPresent?(slotIndex: number): boolean;
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
  if (!reader.enterField(name)) {
    throw new Error(`missing required field ${name}`);
  }
  try {
    if (reader.isNull()) {
      throw new Error(`missing required field ${name}`);
    }
    return convert(reader);
  } finally {
    reader.exit();
  }
}

function optionalField<T>(reader: RuntimeDataReader, name: string, convert: (value: RuntimeDataReader) => T): T | undefined {
  if (!reader.enterField(name)) {
    return undefined;
  }
  try {
    return reader.isNull() ? undefined : convert(reader);
  } finally {
    reader.exit();
  }
}

function readRequiredInt64(reader: RuntimeDataReader, name: string): number {
  if (reader.isPositional?.()) {
    return reader.readNextInt64!();
  }
  return requiredField(reader, name, (value) => value.asNumber());
}

function readRequiredFloat64(reader: RuntimeDataReader, name: string): number {
  if (reader.isPositional?.()) {
    return reader.readNextFloat64!();
  }
  return requiredField(reader, name, (value) => value.asNumber());
}

function readRequiredBool(reader: RuntimeDataReader, name: string): boolean {
  if (reader.isPositional?.()) {
    return reader.asBool();
  }
  return requiredField(reader, name, (value) => value.asBool());
}

function readRequiredString(reader: RuntimeDataReader, name: string): string {
  if (reader.isPositional?.()) {
    return reader.asString();
  }
  return requiredField(reader, name, (value) => value.asString());
}

function readOptionalInt64(reader: RuntimeDataReader, name: string): number | undefined {
  if (reader.isPositional?.()) {
    return reader.readOptionalPresent!(0) ? reader.readNextInt64!() : undefined;
  }
  return optionalField(reader, name, (value) => value.asNumber());
}

function readOptionalFloat64(reader: RuntimeDataReader, name: string): number | undefined {
  if (reader.isPositional?.()) {
    return reader.readOptionalPresent!(0) ? reader.readNextFloat64!() : undefined;
  }
  return optionalField(reader, name, (value) => value.asNumber());
}

function readOptionalBool(reader: RuntimeDataReader, name: string): boolean | undefined {
  if (reader.isPositional?.()) {
    return reader.readOptionalPresent!(0) ? reader.asBool() : undefined;
  }
  return optionalField(reader, name, (value) => value.asBool());
}

function readOptionalString(reader: RuntimeDataReader, name: string): string | undefined {
  if (reader.isPositional?.()) {
    return reader.readOptionalPresent!(0) ? reader.asString() : undefined;
  }
  return optionalField(reader, name, (value) => value.asString());
}

"#,
    );
}
