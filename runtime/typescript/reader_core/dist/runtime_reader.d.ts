export declare const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: "^1.0";
export interface RuntimeDataReaderLike {
    enterField(name: string): boolean;
    enterIndex(index: number): void;
    exit(): void;
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
    private readonly stack;
    constructor(value: unknown);
    enterField(name: string): boolean;
    enterIndex(index: number): void;
    exit(): void;
    length(): number;
    asNumber(): number;
    asBool(): boolean;
    asString(): string;
    isNull(): boolean;
    private currentValue;
}
