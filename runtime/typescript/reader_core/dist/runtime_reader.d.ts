export declare const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: "^1.0";
export interface RuntimeDataReaderLike {
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
export interface RuntimeDecodeLimits {
    maxFileBytes: number;
    maxRows: number;
    maxArrayLength: number;
    maxStringBytes: number;
    maxNestingDepth: number;
}
export declare const DEFAULT_RUNTIME_DECODE_LIMITS: RuntimeDecodeLimits;
export type RuntimeDecodeLimitsInput = Partial<RuntimeDecodeLimits>;
export declare function normalizeRuntimeDecodeLimits(limits?: RuntimeDecodeLimitsInput): RuntimeDecodeLimits;
export declare function checkRuntimeDecodeLimit(label: string, actual: number, limit: number): void;
export interface RuntimeDataLoaderLike {
    open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
}
export type RuntimeReaderDecoder = (bytes: Uint8Array, path: string) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
export declare class RuntimeValueReader implements RuntimeDataReaderLike {
    private readonly stack;
    private readonly limits;
    constructor(value: unknown, limits?: RuntimeDecodeLimitsInput);
    enterField(name: string): boolean;
    enterIndex(index: number): void;
    exit(): void;
    length(): number;
    enterRequiredField(name: string): void;
    enterOptionalField(name: string): boolean;
    exitField(): void;
    readRowCount(): number;
    readListLength(): number;
    readEnumIndex(variants: readonly string[]): number;
    arrayElementsArePositional(): boolean;
    beginRecord(_optionalSlotCount: number): void;
    endRecord(): void;
    asInt64(): number;
    asFloat64(): number;
    asBool(): boolean;
    asString(): string;
    ensureExhausted(): void;
    private asFiniteNumber;
    private currentValue;
    private checkNestingDepth;
}
