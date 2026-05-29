# 13. Runtime Binary Format 详细设计

本文档描述 ConfigForge 私有 runtime data 二进制格式（规范格式名 `bin`，默认后缀 `.bin`）的目标设计与分阶段实现方案。该格式面向运行时加载效率和包体体积，牺牲自描述性：文件内容不保存 table name、field name、fragment name、enum name 等字符串标识，所有数据严格按照 exporter 与 generated code 共享的 Meta 顺序紧密排列。

产品行为仍以 `Documentation/Design/06-export-cli.md` 为入口；本文件只约束 `bin` 格式的私有二进制编码、schema revision、Exporter / Codegen / RuntimeDataReader 协作方式和测试边界。

## 13.1 目标与非目标

目标：

1. 新增 runtime data format：`format = "bin"`，默认输出 `<table>.bin`。
2. `bin` 文件头只写入最小格式识别信息和 schema revision；不写 table name、field name、fragment name、enum name、tag 或其他 payload 自描述元信息。
3. revision 由 Meta 中影响二进制布局和 typed reader 的关键信息稳定 hash 得到，不由用户手工配置。
4. Exporter 与 Codegen 使用同一套 target-aware、schema-aware 顺序定义，确保生成代码按相同顺序读取。
5. RuntimeDataReader contract 支持 named-field layout（JSON / MessagePack）和 positional packed layout（bin），生成代码的 typed row/object 构造逻辑尽量复用。
6. 官方 runtime helper / reader adapter 后续覆盖 Rust、TypeScript、C++ 的 `bin` 读取能力。

非目标：

- `bin` 不是跨语言通用公开标准，不承诺第三方无需 generated code 即可解析。
- `bin` v1 不做 schema evolution 兼容读取；revision 不一致直接报错。
- `bin` v1 不做随机字段访问、列式存储、压缩块、加密、校验和或索引页。
- `bin` v1 不解决数据热更新 diff；它是单 table artifact 的完整快照。
- `bin` v1 不改变 JSON / MessagePack 的现有 named payload shape。

## 13.2 文件整体结构

每个 table 仍输出一个 runtime data artifact。文件结构：

```text
magic[4] = "CFGB"
u16 format_version_le = 1
u16 header_size_le = 16
u64 schema_revision_le
table_payload
```

说明：

- 文件头固定 16 字节，小端序。
- `magic = "CFGB"` 只用于快速识别 ConfigForge binary artifact 和稳定错误分类，不代表格式自描述；payload 仍完全依赖 generated code 的 schema layout。
- `format_version = 1` 表示当前 binary wire contract。reader 看到不支持的 version 必须报 unsupported binary format version，而不是继续按 revision 尝试读取。
- `header_size = 16` 为 v1 固定值。v1 reader 只接受 16；未来若扩展 header，必须通过新的 format version 明确升级 reader contract。
- 格式选择仍由 `TargetDataConfig.format = "bin"` 和 generated code 中的 data format 视图决定；reader 不应把 magic 当作通用自动 schema 发现机制。

### 13.2.1 CollectionTable payload

```text
varuint row_count
row_0
row_1
...
row_n
```

行顺序与现有 exporter 相同：经过 target table visibility、row_export 和 validation/normalization 后，保留 source order。每行内部按 table schema layout 顺序写出字段和 fragment。

### 13.2.2 ObjectTable payload

```text
object_record
```

ObjectTable 已经由 validation/normalization 保证最多一个 logical row。`bin` 不写 rows envelope，也不写 object 字段名；object record 只按 schema layout 写当前 target 可见字段。如果 table 可见但所有字段都被 target 过滤，文件只有 16 字节 header，后续没有 record bytes。

## 13.3 Schema layout 与 revision

`bin` 的核心契约是：Exporter 和 generated code 必须从同一份 linked Meta 和 target tags 得到完全一致的 schema layout。

### 13.3.1 Layout 输入

对每个 table artifact 构造 target-specific layout，包含：

1. table canonical name 与 table kind（Collection / Object）。
2. 当前 target 可见的 top-level fields，按 Meta 声明顺序。
3. 当前 target 可见的 fragments，按 Meta 声明顺序；每个 fragment 内字段按声明顺序。
4. 每个字段的类型信息：primitive、enum name + enum values 顺序、struct name + recursive child layout、array/map element type（未来启用时）。
5. 每个字段/fragment 的 required/optional 语义，以及是否需要 presence bit。
6. 会影响字段集合或读取布局的策略摘要，例如 table/field/fragment `export_when` 解析后的可见性。
7. `bin` contract version 字符串（例如 `config-forge-bin-v1`），避免同一 schema 在不同二进制编码规则下碰撞。

不进入 layout 的信息：

- `desc`、文档注释、source path、CSV header alias、生成代码命名策略。
- 实际数据值、行数、artifact 输出路径。
- `row_export` 等只影响导出行集合的数据选择策略。它会改变 `row_count` 和 rows 内容，但不改变 generated parser 的 positional layout，不进入 `BinarySchemaRevision`；如未来需要追踪可放入 manifest / debug metadata。

### 13.3.2 Canonical schema signature

实现应先把 layout 编码成稳定 canonical byte stream，再计算 revision。必须使用无歧义的二进制 token stream，而不是 Debug/TOML/JSON 字符串，也不使用 NUL 分隔文本。

```text
u8 token_kind
varuint payload_len
payload_bytes
```

token payload 规则：

- 字符串统一编码为 `varuint utf8_len + utf8_bytes`，不依赖任何字符分隔符。
- 数字使用固定 little-endian 或同本文档一致的 varuint；每种 token 的 payload schema 固定。
- 复合结构必须显式写 begin/end token 或 child_count，避免递归边界依赖外部语境。
- enum values、struct children、array element type 都递归写入。

虽然 `.bin` 数据文件不保存 name，revision hash 输入必须包含 table/field/fragment/enum/struct canonical name，以便重命名后旧 reader 能发现 schema 不匹配。

### 13.3.3 Revision hash 算法

建议 v1 使用稳定、无外部依赖也易于跨语言实现的 `FNV-1a 64-bit`：

```text
offset_basis = 14695981039346656037
prime        = 1099511628211
for byte in canonical_signature:
    hash ^= byte
    hash *= prime   // wrapping u64
```

输出作为 `u64 little-endian` 写入文件头，并作为 generated code 中每张 table 的 `EXPECTED_SCHEMA_REVISION` 常量。FNV-1a 不是安全 hash；这里的目标是快速发现 exporter/generated code schema 不一致，而不是防篡改。若未来需要极低碰撞概率，可在 bin v2 改为 128-bit revision；v1 为了保持 revision 字段和跨语言实现简单选择 `u64`。

## 13.4 值编码规则

所有多字节固定宽度数值使用 little-endian。所有 record 紧密排列，不做对齐填充。

### 13.4.1 Unsigned varint

`varuint` 使用 ULEB128：低 7 bit 为 payload，最高 bit 表示后续字节。用于 row_count、array length、string byte length、enum ordinal 等非负整数。

### 13.4.2 Signed int

Meta `int` 使用 ZigZag + ULEB128：

```text
zigzag(i64) = (value << 1) ^ (value >> 63)
write varuint(zigzag)
```

理由：配置表中小整数和少量负数常见，varint 比固定 `i64` 更紧凑；ZigZag 避免负数总是占满 10 字节。

跨语言 runtime contract：Meta `int` 在 Rust / C++ 中分别映射为 64-bit signed integer，在 TypeScript 中映射为 `number`，不使用 `bigint`。因此 `bin` v1 要求所有导出的 `int` 值都必须位于 JavaScript safe integer 范围 `[-(2^53 - 1), 2^53 - 1]` 内；binary exporter 必须在写入前拒绝超出范围的值，TypeScript binary reader 也必须在 decode 时校验并报错。若未来需要超过 safe integer 的整数，应新增显式 primitive type（例如大整数专用类型）及独立 codegen/runtime contract，而不是改变现有 `int` 的 TypeScript 表达。

### 13.4.3 Float

Meta `float` v1 使用 IEEE-754 `f64` little-endian 固定 8 字节。

理由：当前 Meta 没有区分 `float32` / `float64`，固定 `f64` 可避免精度隐式丢失。未来如增加 `float32` 类型，可在 type signature 和 value encoding 中新增 4 字节编码。

v1 浮点只允许有限值。validation 应禁止 `NaN`、`Infinity`、`-Infinity` 等 non-finite float；binary exporter 如果仍收到 non-finite value，应作为 validation/export invariant violation 报错，不得生成 `.bin`。这样可避免不同语言对 NaN bit pattern、比较和 JSON / MessagePack 表达产生分歧。

### 13.4.4 Bool

单个 bool 使用 1 byte：`0x00 = false`，`0x01 = true`，其他值为 decode error。

暂不做 bit-packing。原因：

- 生成代码按字段顺序逐个读取时，byte bool 简单且错误定位清晰。
- bit-packing 对 mixed record 字段会引入跨字段状态，影响 reader cursor 简洁性。
- presence bitmap 已经承担 optional/fragment 是否存在的压缩职责。

未来可仅对 `array<bool>` 增加专用 bitset 编码，但这应作为 bin v2 或显式 array subtype 优化。

### 13.4.5 String / Text / Asset

```text
varuint byte_len
utf8_bytes
```

`string`、`text`、`asset` 均按 UTF-8 bytes 写入。Reader 必须验证 UTF-8 合法性，错误中包含 table/field/row 上下文（由 generated parser 提供）。

### 13.4.6 Enum

Enum 使用 `varuint ordinal`，ordinal 是 enum value 在 Meta 声明顺序中的 0-based index。

理由：不写 enum string 可显著减少重复数据；revision signature 包含 enum name 和 value 顺序，因此 enum 重排/重命名会导致 revision 不匹配。

Exporter 端必须使用 linked enum definition 将 resolved/source enum string 映射为 ordinal。找不到值时应产生 structured diagnostic，包含 table、field、row、enum name 和原始值；不得 panic 或退化为 generic encode error。validation 应保证非法 enum source 值不能进入 `.bin` 生成路径，binary exporter 是最后一道一致性防线。

### 13.4.7 Struct

Struct 按 recursive struct layout 写 record：

```text
presence_bitmap_for_optional_children
required_child_0_or_present_optional_child_0
required_child_1_or_present_optional_child_1
...
```

Struct 自身是否存在由父字段或 fragment 的 presence bit 表达；一旦存在，内部 children 按自身 required/optional 规则编码。

### 13.4.8 Array

Array 编码遵循用户指定规则：先写长度，再逐个元素写入。

```text
varuint len
element_0
element_1
...
element_n
```

元素编码使用其 element type 的标准编码。数组元素是否允许 null / optional 需要由未来 Meta array 语义明确；v1 建议 array element 不单独支持 null，若需要可用 element presence bitmap 的 array subtype 在后续版本设计。

### 13.4.9 Map（未来）

Map 不作为当前 bin v1 必需能力。若未来启用，建议：

```text
varuint entry_count
key_0
value_0
key_1
value_1
...
```

key/value 类型必须来自 Meta，entry 顺序必须 deterministic；如果 source 是无序 map，exporter 应按 key 的 binary canonical order 排序后写入。

## 13.5 Record presence bitmap

为了不写 field name，同时表达 optional field / optional fragment / optional struct occurrence，record 在字段值之前写 presence bitmap。

规则：

1. 对当前 record layout 中的 immediate optional slots 编号，不递归覆盖嵌套 struct 内部 optional child。
2. bitmap byte 数为 `ceil(optional_slot_count / 8)`。
3. bit index 使用 LSB-first：slot 0 是 byte 0 的 bit 0。
4. bit = 1 表示该 optional slot 的 value bytes 随后出现；bit = 0 表示 absent，不写 value bytes。
5. required slot 不占 bitmap bit，value 必须写入。
6. optional slot 顺序是当前 record layout 中 direct children 的 schema order；required slot 不占 bit。
7. optional slot present 但其内部为空 struct 时，仍写 bit = 1，然后写该 struct record 自己的 bitmap/children。

顶层 table record、fragment record、struct record 都遵循同一规则，但 bitmap 只覆盖各自 record 的直接 optional slots。若某个 present optional slot 的值类型是 struct，则随后写入该 struct record；该 struct record 再写自己的 presence bitmap。这样可以避免“顶层 bitmap 递归覆盖全部 optional slots”和“每层 record 单独 bitmap”两种实现分叉。

Fragment 在 binary layout 中表现为 table record 的一个 direct slot：fragment 自身是否出现由 table record 的 fragment presence bit 表示；fragment 内部字段若需要 optional 表达，则进入 fragment record，由 fragment record 自己的 bitmap 覆盖 direct fields。若未来改为 fragment fields 展开到 table record，必须作为新的 binary contract version 处理。

这样 generated parser 不需要字段名，也不需要 sentinel/null tag，即可按 Meta 顺序准确跳过 absent value。

## 13.6 Typed binary value source

JSON / MessagePack v1 保持现有 named、string-compatible runtime payload contract；`bin` 是 schema-aware typed binary encoding。二者共享 target-aware export 语义，但 wire scalar contract 分层：

1. JSON / MessagePack 继续输出现有 named payload shape，scalar 可保持 string-compatible 表达，由 generated parser/helper 做字符串到 int/bool/float/enum 的转换。
2. `bin` exporter 必须依据 Meta type 将 resolved value 编码为 typed bytes：int 使用 ZigZag varuint 且必须在 JavaScript safe integer 范围内，float 使用有限 `f64`，bool 使用 0/1 byte，enum 使用 ordinal。
3. int / float / bool / enum 的接受范围和 parse 规则必须复用 validation / generated parser helper 的同一套语义，避免三种格式对同一 cell 接受范围不同。
4. validation 应保证 binary exporter 不会遇到非法 typed value；如果仍遇到，应作为 exporter diagnostic 或 internal invariant violation 明确报错，不能生成部分 `.bin`。

## 13.7 Exporter 实现方案

建议新增：

```text
src/exporter/
  binary.rs           # BinaryDataExporter / bin revision / encoder
```

核心 API：

```rust
pub const FORMAT: &str = "bin";

pub struct BinaryDataExporter;

impl DataFormatExporter for BinaryDataExporter {
    fn name(&self) -> &'static str { FORMAT }
    fn aliases(&self) -> &'static [&'static str] { &["binary"] }
    fn extension(&self) -> &'static str { "bin" }
    fn serialize_table(...) -> Result<Vec<u8>>;
}
```

实现步骤：

1. 从 `LinkedTableMeta` + `TargetTags` 构造 `BinaryTableLayout`。
2. 计算 `schema_revision(layout)` 并写入 16 字节 file header。
3. 继续复用 `DataResolver::resolve_table_rows / resolve_table` 获取 normalized resolved values。
4. CollectionTable 写 `row_count` + rows；ObjectTable 只写 object record。
5. 按 layout 顺序从 `ResolvedValue` 取值并编码；如果 required value 缺失，返回 validation-style error。
6. Registry 中注册 `BinaryDataExporter`，Project Config 支持 `format = "bin"`。

不要复用 `ExportedTablePayload` 直接 serde 序列化，因为 text payload 是 named map/object shape，而 `bin` 需要 typed positional layout。建议抽出一个共享的 `RuntimeExportView` / `ResolvedExportTable` helper，JSON、MessagePack 和 Binary 都从同一份 target-aware resolved view 构建：

- JSON / MessagePack：把 view 渲染为现有 named payload。
- Binary：把 view 按 `BinaryTableLayout` 渲染为 positional bytes。

这样可满足“三种格式使用同一套导出语义”，同时允许不同 wire layout。

## 13.8 Codegen 与 RuntimeDataReader 方案

### 13.8.1 CodeModel 需要携带 format 信息

当前 codegen 主要通过 `RuntimeDataFile.path` 得到数据文件名。支持 `bin` 前必须显式增加 runtime data format 视图，避免仅靠后缀推断：

```rust
pub struct RuntimeDataFile {
    pub table_name: String,
    pub path: String,
    pub format: String, // json / messagepack / bin
}
```

`format` 是 CodeModel 构建和 codegen validate 的必需输入；unknown format 应在 build/codegen 阶段报错，不应等到 generated runtime 才失败。后缀只能作为 artifact path 规划结果，不能作为 CodeModel format 的隐式来源。

### 13.8.2 Generated parser 分层

生成代码应保持一套 typed row/object 构造逻辑，但读取入口按 data format 选择 reader layout：

```text
load table file
  -> create RuntimeDataReader adapter for json/messagepack/bin
  -> generated parser calls schema-driven field APIs
  -> parse table envelope/record
  -> construct typed rows/object
```

JSON / MessagePack reader 提供 named-field layout：

- collection：读取 `rows` array。
- object：读取 top-level object fields。
- field：按 Meta name 查找。

Binary reader 提供 positional packed layout：

- 读取 16 字节 header，校验 magic / format_version / header_size 后，再读取 `schema_revision` 并和 generated const 比较。
- collection：读取 row_count。
- object/row：按 generated schema order 读取 presence bitmap 和字段值。
- field：不按名字查找，只按生成代码调用顺序读取。

### 13.8.3 Reader contract 扩展

RuntimeDataReader contract 使用 schema-driven field APIs 消化 named / positional 差异，generated parser 不再通过 `is_positional` / `IsPositional` / `isPositional` 判断 reader layout。建议接口形态：

```text
schema_revision() -> Option<u64>
read_row_count()
begin_record(optional_slot_count)
enter_required_field(name)
enter_optional_field(name) -> bool
exit_field()
read_enum_index(variants) -> usize
read_optional_present()
read_<type>()
ensure_exhausted()
```

各语言可以按语言习惯命名，但语义必须一致。JSON / MessagePack adapter 的 `enter_required_field` / `enter_optional_field` 按字段名进入对象字段，`exit_field` 回退 cursor，`read_enum_index` 将 enum string 映射到 generated variants 下标。Binary adapter 的 `enter_required_field` 是 no-op，`enter_optional_field` 消费当前 record presence bitmap，`exit_field` 是 no-op，`read_enum_index` 读取 enum ordinal 并校验范围。低层 named cursor API 和 positional sequential API 可以继续作为 adapter 实现细节保留，但 generated parser 不应直接按 layout 分支读取字段。

### 13.8.4 标准 runtime helper 覆盖矩阵

`runtime/` 标准 helper 后续增加：

| 语言 | bin reader 形态 |
| --- | --- |
| Rust | `binary_reader` / `binary_fs_loader` copyable template；无第三方依赖。 |
| TypeScript | `@config-forge/runtime-binary` value/cursor reader；支持 `ArrayBuffer` / `Uint8Array`。 |
| C++ | header-only 或小型源文件 `BinaryRuntimeDataReader`；只依赖标准库。 |

binary reader 应共享 compliance fixtures：

- varuint / zigzag 边界值；
- UTF-8 错误；
- optional bitmap；
- enum ordinal 越界；
- revision mismatch；
- row_count 与实际 bytes 不匹配；
- trailing bytes 检测。

## 13.9 兼容性与错误处理

- invalid magic：报 invalid binary runtime data magic，包含 path 和实际前 4 字节（可安全展示时）。
- unsupported format version / header_size：报 unsupported binary format version/header，包含 path、actual、supported。
- `schema_revision` mismatch：生成代码必须报错，包含 table 名、文件 path、expected revision、actual revision。
- 文件过短：报 truncated binary runtime data。
- varuint 超过目标整数范围或超过 10 字节：报 malformed varuint。
- bool 非 0/1：报 invalid bool byte。
- enum ordinal 超出 generated enum value 数量：报 invalid enum ordinal。
- string length 超出剩余 bytes 或 UTF-8 非法：报 invalid string。
- record 解析完成后若存在 trailing bytes：报 trailing bytes，避免 silent schema drift。

所有 decode error 都应尽量带 structured context：table、field/fragment path、row index（CollectionTable）、byte offset、expected type、actual raw value/length。对于 enum、float、UTF-8、bool 等 typed scalar 错误，错误消息必须能定位到具体字段，而不是只有 cursor offset。

### 13.9.1 Decode limits 与资源上限

标准 binary reader/helper 必须提供默认 decode limits，并允许业务覆盖。reader 不能在验证前按 untrusted count 直接预分配巨大数组。

建议默认规则：

- `varuint` 最多 10 字节，超过目标整数范围时报 malformed varuint。
- string byte length 必须 `<= remaining_bytes`，且 `<= max_string_bytes`。
- `row_count` / array length 必须 `<= remaining_bytes` 推导出的理论最大元素数；对固定宽度或最小 1 字节元素可做保守上界检查。
- collection rows / arrays 应按 streaming 或渐进扩容读取；不得在上限校验前一次性 reserve untrusted count。
- nested record / array depth 应受 `max_nesting_depth` 限制，避免恶意数据触发深递归。

`bin` 不做字段级宽松兼容。新增/删除/重排/重命名字段、修改 enum 顺序、修改 required/optional、修改 struct shape 都会改变 revision，旧 generated code 不能读取新文件。

## 13.10 测试计划

Exporter 单元测试：

1. `DataFormatRegistry::with_builtin_exporters()` 能解析 `bin` 和 `binary` alias。
2. `BinaryDataExporter.extension() == "bin"`，artifact path 为 `<table>.bin`。
3. 同一 Meta + target tags 重复导出得到相同 revision 和 bytes。
4. 字段顺序变化、字段类型变化、enum value 顺序变化会改变 revision。
5. `row_export` 仅改变导出行集合时不改变 schema revision。
6. CollectionTable row_count + rows 顺序正确。
7. ObjectTable 只写 header + object record，不写 rows envelope。
8. optional field / fragment presence bitmap 覆盖 present/absent/empty struct。
9. golden bytes 覆盖“顶层 optional struct present，struct 内部 required child + absent optional child”，锁定 bitmap 层级。
10. int/float/bool/string/enum/struct/array 编码边界值。
11. non-finite float、非法 enum source 值不能生成 `.bin`，诊断包含字段上下文。

Codegen / runtime helper 测试：

1. Rust / TypeScript / C++ generated code 能加载 `.bin` runtime data。
2. revision mismatch 诊断包含 expected/actual。
3. magic/version/header_size 错误有独立诊断，不被误报为 revision mismatch。
4. JSON / MessagePack / bin 三种格式对同一 Meta 构造出的 typed rows/object 等价。
5. 标准 runtime helper compliance suite 覆盖 malformed binary 输入和 decode limits。

跨语言 canonical fixtures：

1. `schema_signature.bin` 或可读 token dump，用于验证 Rust / TypeScript / C++ 计算出的 canonical signature 与 revision 完全一致。
2. `varuint_zigzag.bin` 覆盖 ULEB128 / ZigZag 边界。
3. `presence_bitmap_nested_struct.bin` 锁定 per-record immediate bitmap 语义。
4. malformed fixtures 覆盖 invalid magic、unsupported version、truncated header、invalid UTF-8、invalid bool、enum ordinal 越界、resource limit exceeded。

集成测试：

1. standard project 增加一个 `format = "bin"` target 或独立 fixture。
2. 构建同时导出 JSON、MessagePack、bin，分别绑定不同 code task。
3. dry-run / temp-commit / direct artifact writer 都能处理 `.bin` artifacts。

## 13.11 分阶段实现建议

### Phase 1：Exporter + revision + bytes fixtures

- 新增 `src/exporter/binary.rs`。
- 实现 16 字节 header、layout、revision hash、primitive/struct/collection/object 编码。
- 注册 `bin` exporter。
- 增加 exporter 单元测试和少量 golden bytes fixture。

### Phase 2：CodeModel format 信息 + generated parser

- `RuntimeDataFile` / CodeModel 中显式记录 runtime data format，作为 `bin` generated parser 前置改造。
- 三个语言生成器按 format 选择 named 或 positional parser。
- generated code 写入每张 table 的 `EXPECTED_SCHEMA_REVISION` 常量。

### Phase 3：标准 runtime binary readers

- Rust / TypeScript / C++ runtime helper 增加 binary reader。
- compliance fixtures 复用同一批 `.bin` bytes。

### Phase 4：端到端与性能验证

- CLI integration 覆盖 `format = "bin"`。
- 与 JSON / MessagePack 比较包体大小与加载耗时。
- 根据数据决定是否增加 string interning、bool array bitset、block compression 等 v2 优化。

## 13.12 待讨论问题

1. `schema_revision` 使用 `u64` 是否足够；若希望更低碰撞概率，可在 bin v2 改为 128-bit revision。
2. `float` 是否需要在 Meta 中区分 `float32` / `float64`。
3. 是否允许 `array<T?>` 或 nullable element；如果允许，需要 array element presence bitmap。
4. 是否在 v1 支持 string table / interning。它能显著压缩重复字符串，但会引入额外段结构，不符合“严格顺序紧密排列”的最小方案，建议先不做。
