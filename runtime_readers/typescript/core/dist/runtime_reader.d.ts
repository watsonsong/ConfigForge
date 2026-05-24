export declare const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: "^1.0";
export interface RuntimeDataReaderLike {
    field(name: string): RuntimeDataReaderLike | undefined;
    index(index: number): RuntimeDataReaderLike;
    length(): number;
    asNumber(): number;
    asBool(): boolean;
    asString(): string;
    isNull(): boolean;
}
export interface RuntimeDataLoaderLike {
    open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
}
export type RuntimeReaderDecoder = (bytes: Uint8Array, path: string) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
export declare class RuntimeValueReader implements RuntimeDataReaderLike {
    private readonly value;
    constructor(value: unknown);
    field(name: string): RuntimeDataReaderLike | undefined;
    index(index: number): RuntimeDataReaderLike;
    length(): number;
    asNumber(): number;
    asBool(): boolean;
    asString(): string;
    isNull(): boolean;
}
