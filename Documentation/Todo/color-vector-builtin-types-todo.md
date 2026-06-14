# Color / Vector Builtin Types Development Todo

目标：为 ConfigForge 增加语言和引擎无关的 `color`、`vector2`、`vector3`、`vector4`、`vector2_int`、`vector3_int`、`vector4_int` builtin value type，并让 runtime data、validation、schema docs、C# .NET / Unity codegen 能基于同一份 Meta / Data 稳定工作。

## Step 1. Meta schema 与配置解析

- 在 `FieldMeta` 中落地 Color / Vector typed options。
- 支持用户侧摊平 TOML 字段：`color_format`、`color_channel`、`color_space`、`alpha`、`vector_format`、`vector_components`、`unit`。
- 在 Meta validate 阶段检查：
  - Color options 只能用于 `type = "color"`。
  - Vector options 只能用于 `vector2/3/4` 与 `vector*_int`。
  - 禁止 enum / struct 与 builtin type 同名。
  - `vector_components` 数量必须匹配维度，组件名在字段内唯一。
  - `color_channel` 必须显式为 `byte` 或 `normalized`，不支持 `auto`。
- 单元测试覆盖合法默认值、非法 option/type 组合、同名类型冲突和诊断位置。

## Step 2. Link model 与 target-aware ExportView

- 扩展 linked field type，明确表达 builtin value type，而不是把它们当成 named struct。
- 确认 target filtering 后 builtin field options 仍随字段进入 ExportView。
- 更新 schema doc / validation report 输入模型，使其能展示 source format、unit、color space 和 alpha policy。
- 单元测试覆盖 builtin type 在 table、struct、ObjectTable、target filtering 中的 link 结果。

## Step 3. Data typed value parser

- 在 `src/data/typed_value.rs` 增加共享解析 helper：
  - `parse_color_field_value()`：支持 `hex_rgb`、`hex_rgba`、`rgb_tuple`、`rgba_tuple`、`object`。
  - `parse_vector_field_value()`：支持 `tuple`、`array`、`object`。
  - `parse_expanded_vector_value()` 或 resolver 侧 expanded collector：支持 `field.x` / `field.y` / `field.z` 等展开列。
- 解析后形成 canonical `DataValue::Color` / `DataValue::Vector`，并保留 raw value 与 source trace。
- 检查范围、finite float、整数向量拒绝小数、维度不匹配、缺失 alpha / component、expanded parent cell 混用。
- 单元测试覆盖 CSV cell、ObjectTable key-value rows、默认值、optional 空值和诊断路径。

## Step 4. Validation 迁移与内置校验能力

- 让 required/type validation 消费 resolved builtin values，避免 validation 和 resolver 双解析。
- 扩展 `range` validator 或新增 vector/color 专用 validator：
  - vector magnitude / component range。
  - color alpha range / color space policy。
- FK/index 第一阶段不允许 Color/Vector 作为 key；如允许必须明确 equality/hash 语义。
- 测试覆盖 validation report 中的 builtin field path 与 source trace。

## Step 5. Runtime data artifact canonical 输出

- 扩展 data artifact toolkit 的 runtime payload model，支持 Color / Vector canonical values。
- JSON / MessagePack 输出建议：
  - Color：`{"r":...,"g":...,"b":...,"a":...}`。
  - Vector：按 field components 输出 object，例如 `{"x":1,"y":2,"z":3}`。
- Binary 输出使用 schema-aware positional layout，避免重复字段名。
- 明确 Color byte / normalized 导出策略；若配置允许，应放在 exporter option，而不是 codegen backend 私有逻辑。
- 回归测试覆盖 JSON、MessagePack、bin 与 Lua table 输出。

## Step 6. CodeModel IR 与通用 codegen options

- 扩展 `CodeTypeRef`：`Color`、`Vector { dimension, scalar }`。
- 在 `CodeField` 或 field options 中携带 `color_space`、`color_channel`、`vector_components`、`unit` 等生成所需信息。
- 增加 builtin type mapping 的通用结构与 symbol collision 检查。
- 更新 Rust / TypeScript / C++ / Java backend 的默认 fallback 类型策略，至少能生成自带 value struct/class/interface，不阻塞非 C# target。
- 梳理并实现 profile 默认矩阵：C# `dotnet/unity/godot`、Unreal C++ `unreal`、TypeScript `standard/cocos_creator`、C++ `standard`、Java `standard`。profile 第一阶段只影响 generated API 类型、构造表达式和 import/include，不承诺 engine runtime adapter。
- Snapshot 测试覆盖所有语言的 fallback mapping。

## Step 7. C# .NET / Unity profile

- 在 `[targets.exports.csharp]` 增加：
  - `profile = "dotnet" | "unity"`。
  - `type_mappings`。
  - `using`。
- `dotnet` 默认：`System.Numerics.Vector2/3/4`，Color 和 int vector 用 generated fallback value type，除非用户显式映射。
- `unity` 默认：`UnityEngine.Color`、`Vector2/3/4`、`Vector2Int/Vector3Int`，`Vector4Int` fallback。
- 生成 parser 构造表达式：`new Vector3(x, y, z)`、`new Color(r, g, b, a)`；必要时处理 byte -> normalized conversion。
- dotnet harness 编译 generated fallback types；Unity profile 做 source assertion，后续可增加 Unity compiler/harness。

## Step 7b. Unreal / Cocos / Godot profile

- `cpp_unreal` 增加/确认 `profile = "unreal"`，默认映射到 `FLinearColor`、`FVector2D`、`FVector`、`FVector4`、`FIntPoint`、`FIntVector`，Blueprint support 下确保 `UPROPERTY` 类型可反射。
- `typescript` 增加 `profile = "standard" | "cocos_creator"`；Cocos Creator profile 默认映射 `Color`、`Vec2`、`Vec3`、`Vec4` 并生成 `import { Color, Vec2, Vec3, Vec4 } from "cc"` 或用户配置 import。
- `csharp profile = "godot"` 默认映射 `Godot.Color`、`Vector2/3/4`、`Vector2I/Vector3I`。
- 通用 C++ / Java 默认 generated fallback，允许项目后续通过 `type_mappings` 指向自研引擎或 Android/LibGDX 数学库。

## Step 8. 标准工程 fixture 与文档同步

- 在 `tests/fixtures/standard_project/` 增加 `ui_themes`、`spawn_points` 或等价游戏表，覆盖 Color / Vector 字段。
- 更新 Design / Implementation / user manual 示例，确保 TOML、CSV、generated API 三者一致。
- `schema_doc` 输出应展示 builtin type、source format、unit、target mapping 摘要。
- CLI integration 测试覆盖 `client_unity` / `server_dotnet` target 的构建路径。

## Step 9. 质量门禁与发布检查

- 运行 `cargo fmt`、`cargo test`，必要时运行 `cargo lint`。
- 对有 `dotnet` 的环境运行 C# compile harness。
- 增加 architecture boundary tests 或更新允许列表，确保新增 builtin type 模块不破坏依赖方向。
- 更新 changelog / release note 时明确：这是新的 builtin value type 能力，不保证与旧的用户自定义 `Color` struct 自动兼容。
