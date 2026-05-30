export declare const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: "^1.0";
export interface RuntimeDataReaderLike {
    enterField(name: string): boolean;
    enterIndex(index: number): void;
    exit(): void;
    length(): number;
    asNumber(): number;
    asBool(): boolean;
    asString(): string;
    schemaRevision?(): bigint | number | undefined;
    decodeLimits?(): RuntimeDecodeLimits;
    enterRequiredField?(name: string): void;
    enterOptionalField?(name: string): boolean;
    exitField?(): void;
    readEnumIndex?(variants: readonly string[]): number;
    readRowCount?(): number;
    readListLength?(): number;
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
    readRowCount(): number;
    readListLength(): number;
    readArrayLength(): number;
    decodeLimits(): RuntimeDecodeLimits;
    asNumber(): number;
    asBool(): boolean;
    asString(): string;
    private currentValue;
    private checkNestingDepth;
}
export declare function readRuntimeRowCount(reader: RuntimeDataReaderLike): number;
export declare function readRuntimeListLength(reader: RuntimeDataReaderLike): number;
