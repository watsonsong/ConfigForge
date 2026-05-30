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
    enterRequiredField?(name: string): void;
    enterOptionalField?(name: string): boolean;
    exitField?(): void;
    readEnumIndex?(variants: readonly string[]): number;
    readRowCount?(): number;
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
export interface RuntimeDataLoaderLike {
    open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
}
export type RuntimeReaderDecoder = (bytes: Uint8Array, path: string) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
export declare class RuntimeValueReader implements RuntimeDataReaderLike {
    private readonly stack;
    constructor(value: unknown);
    enterField(name: string): boolean;
    enterIndex(index: number): void;
    exit(): void;
    length(): number;
    asNumber(): number;
    asBool(): boolean;
    asString(): string;
    private currentValue;
}
