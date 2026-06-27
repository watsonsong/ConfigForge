// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::code_artifact::CodeWriterLike;

pub(super) fn render_runtime_reader_interfaces(content: &mut impl CodeWriterLike) {
    content.raw(
        r#"export type RuntimeDataErrorKind =
  | "Io"
  | "Decode"
  | "SchemaMismatch"
  | "TypeMismatch"
  | "NotFound"
  | "Unsupported"
  | "InvalidPath";

export interface RuntimeDataErrorContext {
  path?: string;
  logicalPath?: string;
  resourcePath?: string;
  expected?: string;
  actual?: string;
}

export class RuntimeDataError extends Error {
  readonly kind: RuntimeDataErrorKind;
  readonly code: string;
  readonly context: RuntimeDataErrorContext;

  constructor(kind: RuntimeDataErrorKind, message: string, context: RuntimeDataErrorContext = {}, options?: { cause?: unknown }) {
    super(message);
    this.name = "RuntimeDataError";
    this.kind = kind;
    this.code = runtimeDataErrorCode(kind);
    this.context = context;
    if (options && "cause" in options) {
      (this as Error & { cause?: unknown }).cause = options.cause;
    }
  }

  static io(message: string, cause?: unknown, context: RuntimeDataErrorContext = {}): RuntimeDataError { return new RuntimeDataError("Io", message, context, { cause }); }
  static decode(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError { return new RuntimeDataError("Decode", message, context); }
  static schemaMismatch(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError { return new RuntimeDataError("SchemaMismatch", message, context); }
  static typeMismatch(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError { return new RuntimeDataError("TypeMismatch", message, context); }
  static notFound(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError { return new RuntimeDataError("NotFound", message, context); }
  static unsupported(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError { return new RuntimeDataError("Unsupported", message, context); }
  static invalidPath(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError { return new RuntimeDataError("InvalidPath", message, context); }
}

export function runtimeDataErrorCode(kind: RuntimeDataErrorKind): string {
  switch (kind) {
    case "Io": return "CF_RT_IO";
    case "Decode": return "CF_RT_DECODE";
    case "SchemaMismatch": return "CF_RT_SCHEMA_MISMATCH";
    case "TypeMismatch": return "CF_RT_TYPE_MISMATCH";
    case "NotFound": return "CF_RT_NOT_FOUND";
    case "Unsupported": return "CF_RT_UNSUPPORTED";
    case "InvalidPath": return "CF_RT_INVALID_PATH";
  }
}

export interface RuntimeDataReader {
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

export interface ConfigForgeColor {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface ConfigForgeVector2 {
  x: number;
  y: number;
}

export interface ConfigForgeVector3 {
  x: number;
  y: number;
  z: number;
}

export interface ConfigForgeVector4 {
  x: number;
  y: number;
  z: number;
  w: number;
}

export interface ConfigForgeVector2Int {
  x: number;
  y: number;
}

export interface ConfigForgeVector3Int {
  x: number;
  y: number;
  z: number;
}

export interface ConfigForgeVector4Int {
  x: number;
  y: number;
  z: number;
  w: number;
}

function isPromiseLike<T>(value: T | Promise<T>): value is Promise<T> {
  return typeof (value as { then?: unknown }).then === "function";
}

function openRuntimeDataSync(loader: RuntimeDataLoader, path: string): RuntimeDataReader {
  const reader = loader.open(path);
  if (isPromiseLike(reader)) {
    throw RuntimeDataError.invalidPath(`runtime loader returned a Promise while synchronously opening ${path}`, { logicalPath: path });
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

function readColor<T = ConfigForgeColor>(reader: RuntimeDataReader, create: (r: number, g: number, b: number, a: number) => T = (r, g, b, a) => ({ r, g, b, a } as T)): T {
  reader.beginRecord(0);
  try {
    const r = readRequiredFloat64(reader, "r");
    const g = readRequiredFloat64(reader, "g");
    const b = readRequiredFloat64(reader, "b");
    const a = readRequiredFloat64(reader, "a");
    return create(r, g, b, a);
  } finally {
    reader.endRecord();
  }
}

function readVectorComponents(reader: RuntimeDataReader, components: readonly string[]): number[] {
  reader.beginRecord(0);
  try {
    return components.map((component) => readRequiredFloat64(reader, component));
  } finally {
    reader.endRecord();
  }
}

function readConfigForgeVector2<T = ConfigForgeVector2>(reader: RuntimeDataReader, components: readonly string[] = ["x", "y"], create: (x: number, y: number) => T = (x, y) => ({ x, y } as T)): T {
  const [x, y] = readVectorComponents(reader, components);
  return create(x, y);
}

function readConfigForgeVector3<T = ConfigForgeVector3>(reader: RuntimeDataReader, components: readonly string[] = ["x", "y", "z"], create: (x: number, y: number, z: number) => T = (x, y, z) => ({ x, y, z } as T)): T {
  const [x, y, z] = readVectorComponents(reader, components);
  return create(x, y, z);
}

function readConfigForgeVector4<T = ConfigForgeVector4>(reader: RuntimeDataReader, components: readonly string[] = ["x", "y", "z", "w"], create: (x: number, y: number, z: number, w: number) => T = (x, y, z, w) => ({ x, y, z, w } as T)): T {
  const [x, y, z, w] = readVectorComponents(reader, components);
  return create(x, y, z, w);
}

function readConfigForgeVector2Int<T = ConfigForgeVector2Int>(reader: RuntimeDataReader, components: readonly string[] = ["x", "y"], create: (x: number, y: number) => T = (x, y) => ({ x, y } as T)): T {
  const [x, y] = readVectorComponents(reader, components);
  return create(Math.trunc(x), Math.trunc(y));
}

function readConfigForgeVector3Int<T = ConfigForgeVector3Int>(reader: RuntimeDataReader, components: readonly string[] = ["x", "y", "z"], create: (x: number, y: number, z: number) => T = (x, y, z) => ({ x, y, z } as T)): T {
  const [x, y, z] = readVectorComponents(reader, components);
  return create(Math.trunc(x), Math.trunc(y), Math.trunc(z));
}

function readConfigForgeVector4Int<T = ConfigForgeVector4Int>(reader: RuntimeDataReader, components: readonly string[] = ["x", "y", "z", "w"], create: (x: number, y: number, z: number, w: number) => T = (x, y, z, w) => ({ x, y, z, w } as T)): T {
  const [x, y, z, w] = readVectorComponents(reader, components);
  return create(Math.trunc(x), Math.trunc(y), Math.trunc(z), Math.trunc(w));
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
