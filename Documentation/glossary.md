# Glossary

本文档记录 ConfigForge 当前设计中的核心术语、中英文命名和推荐代码符号。术语以 `Documentation/Design/` 下的收敛版设计为准。

## 命名原则

- 使用 `Meta` 表示数据结构定义，不使用泛化的 `Config` 指代表结构定义。
- 使用 `Project Config` 表示 ConfigForge 工程入口配置。
- 使用 `ObjectTable` 表示导出一个常量对象的表。
- 使用 `CollectionTable` 表示导出多条记录的集合表。
- 使用 `TableKind` 作为表类型区分字段。
- 使用 `Field` 作为 ObjectTable 和 CollectionTable 共享的基础 Meta 单元。
- 使用 `FieldType` 表示逻辑值类型；使用 `layout` 表示复合值在 source 中的书写方式。
- 使用 `scalar_cell`、`json_cell`、`expanded_value` 描述复合值书写方式，不再使用 `expanded_columns` / `expanded_rows` 作为 canonical layout 名称。
- 使用 `DataSet` 表示 source 读取、组合、覆盖后的完整逻辑数据集合。
- 使用 `ExportView` 表示针对某个 target/context 派生的过滤后导出视图，不再使用 `ExportDataset` 作为正式术语。
- 使用 `PrimaryIndex` / `primary_index` 表示主索引；不再使用 `PrimaryKey` / `primary_key`。
- 使用 `sources` 表示基础 source，使用 `override_sources` 表示覆盖 source；覆盖顺序由列表顺序决定，不再使用 `priority`。
- 使用 `desc` 表示进入 MetaModel 的结构化描述，可用于 inspect、代码注释和辅助文档。
- 使用 `text` 表示国际化文本，使用 `string` 表示普通非本地化字符串。
- `text` 的 POT source reference 默认使用 source 表名和行列号；额外映射列/行可提供 translator comment。
- 使用 `asset` 表示资源引用。
- 常见数学/游戏值优先使用 builtin value type，例如 `color_rgba`、`vector3`、`rotation`、`datetime`。
- 颜色通道策略只允许明确值，例如 `byte` 或 `normalized`；不使用 `auto`。
- 目标语言或引擎类型名属于 target type mapping，不写入逻辑 Meta。
- 诊断输出必须使用稳定 diagnostic code，并保持确定性排序。

## 核心术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 工程 | Project | `Project`, `ProjectConfig` | 一个 ConfigForge 工程，包含 Meta、Source、Target、Plugin 等配置。 |
| 工程配置 | Project Config | `ProjectConfig` | ConfigForge 工程入口配置，通常是 `ConfigForge.toml`。 |
| 版权信息 | Copyright | `copyright` | Project-level 文本元信息，可用于生成代码、schema、manifest 等文件头。 |
| Meta / 元定义 | Meta | `Meta` | 数据结构定义，包含 table、field、type、index、foreign key、validator 等。 |
| Meta 文件 | Meta File | `MetaFile` | 物理 Meta 文件，格式可以是 TOML 或 XML。 |
| Meta 文档 | Meta Document | `MetaDocument` | 一个解析前的 Meta 文档单元。 |
| Meta 模型 | Meta Model | `MetaModel` | 多个 Meta 片段 parse、merge、link 后的 canonical 内存模型。 |
| 配置导入 | Config Import | `ProjectConfig.imports` | 一个 Project Config 片段引用其他工程配置片段；用于统一组合 Meta 定义和 target 定义。 |
| 描述 | Description | `desc` | Meta 各层级定义上的结构化描述，用于 inspect、代码注释和辅助文档。 |
| 表 | Table | `Table`, `TableMeta` | 被 Meta 描述并导出的逻辑配置表。 |
| 表类型 | Table Kind | `TableKind` | 表类型区分字段，例如 object 或 collection。 |
| 对象表 | Object Table | `ObjectTable`, `TableKind::Object` | 导出一个常量对象；常用于全局配置。 |
| 集合表 | Collection Table | `CollectionTable`, `TableKind::Collection` | 导出多条记录；常用于 item、level、quest 等配置。 |
| 字段 | Field | `Field`, `FieldMeta` | Meta 中的基础值定义单元。 |
| 字段名 | Field Name | `FieldName`, `name` | 字段稳定标识。 |
| 字段路径 | Field Path | `FieldPath`, `field_path` | 嵌套字段路径，例如 `reward.item_id` 或 `rewards[0].count`。 |
| 字段类型 | Field Type | `FieldType` | 字段逻辑值类型，例如 `int`、`text`、`vector3`、`struct`、`array`。 |
| 字段默认值 | Default Value | `default` | source 未提供值时使用的默认值。 |
| 必填 | Required | `required` | 当前验证视图中必须有值；无值且无 default 时报错。 |
| 导出字段 | Runtime Field | `RuntimeField` | 会进入常规 runtime data 的字段。 |
| 工具字段 | Tool-Only Field | `tool_only`, `ToolOnlyField` | 仅用于工具流程，默认不进入 runtime data。 |
| 过滤字段 | Filter Field | `filter_field`, `FilterField` | 允许被 export filter 引用的字段。 |
| 仅校验字段 | Validator-Only Field | `validator_only`, `ValidatorOnlyField` | 仅用于 validator，默认不进入 runtime data。 |
| 聚合数据导出策略 | Aggregate Data Export Policy | `data_exports` | 字段是否可被 POT、asset manifest 等聚合 data exporter 作为辅助字段读取。 |

## 字段类型术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 基础类型 | Primitive Type | `PrimitiveFieldType` | `int`、`float`、`bool`、`string`、`text`、`asset` 等。 |
| 整数 | Integer | `FieldType::Int`, `int` | 整数值。 |
| 浮点数 | Float | `FieldType::Float`, `float` | 浮点值。 |
| 布尔值 | Boolean | `FieldType::Bool`, `bool` | 布尔值。 |
| 普通文本 | String | `FieldType::String`, `string` | 非国际化字符串。 |
| 国际化文本 | Text | `FieldType::Text`, `text` | 参与 i18n / POT 流程的文本。 |
| 资源引用 | Asset Reference | `FieldType::Asset`, `AssetRef`, `asset` | 资源路径或资源 ID，可被 asset manifest 使用。 |
| 内置值类型 | Builtin Value Type | `BuiltinValueType` | 常用游戏值类型，不需要项目自定义 struct。 |
| RGB 颜色 | Color RGB | `ColorRgb`, `color_rgb` | 三通道逻辑颜色。 |
| RGBA 颜色 | Color RGBA | `ColorRgba`, `color_rgba` | 四通道逻辑颜色。 |
| 颜色通道策略 | Color Channel Policy | `ColorChannelPolicy`, `color_channel` | 颜色通道解释方式，例如 `byte` 或 `normalized`。 |
| 颜色空间 | Color Space | `ColorSpace`, `color_space` | 颜色空间元信息，例如 `srgb` 或 `linear`。 |
| 二维向量 | Vector2 | `Vector2`, `vector2` | 二维浮点向量。 |
| 三维向量 | Vector3 | `Vector3`, `vector3` | 三维浮点向量，优先作为 builtin value type。 |
| 四维向量 | Vector4 | `Vector4`, `vector4` | 四维浮点向量。 |
| 整数二维向量 | Vector2Int | `Vector2Int`, `vector2_int` | 二维整数向量。 |
| 整数三维向量 | Vector3Int | `Vector3Int`, `vector3_int` | 三维整数向量。 |
| 欧拉旋转 | Rotation | `Rotation`, `rotation` | Euler 旋转值。 |
| 旋转单位 | Rotation Unit | `RotationUnit`, `rotation_unit` | `degrees` 或 `radians`。 |
| 旋转顺序 | Rotation Order | `RotationOrder`, `rotation_order` | Euler 旋转顺序。 |
| 日期时间 | DateTime | `DateTimeValue`, `datetime` | 日期时间值，可带 pattern 和 timezone。 |
| 日期 | Date | `DateValue`, `date` | 只有日期。 |
| 时间 | Time | `TimeValue`, `time` | 只有时间。 |
| 持续时间 | Duration | `DurationValue`, `duration` | 时间跨度。 |
| 整数区间 | Integer Range | `IntRange`, `int_range` | 整数范围。 |
| 浮点区间 | Float Range | `FloatRange`, `float_range` | 浮点范围。 |
| 矩形 | Rect | `Rect`, `rect` | 二维矩形。 |
| 三维包围盒 | Bounds3 | `Bounds3`, `bounds3` | 三维包围盒。 |
| 枚举 | Enum | `EnumDef`, `FieldType::Enum` | Meta 中声明的有限值集合。 |
| 枚举值 | Enum Value | `EnumValue`, `EnumVariant` | 枚举中的单个可用值，可带 `desc`。 |
| 命名结构体 | Named Struct | `NamedStruct`, `StructDef` | 可复用结构体定义。 |
| 内联结构体 | Inline Struct | `InlineStruct` | 字段内部直接定义的一次性结构。 |
| 结构体字段 | Struct Field | `StructField`, `FieldMeta` | struct 内部字段。 |
| 列表 | List | `LinkedField::List`, `read_list_len`, `readListLength`, `ReadListLength` | Schema / Meta / IR / generated helper 层的重复值集合 canonical 术语。`T[]` 是输入语法别名；生成代码读取字段列表长度时必须使用 list length 契约，不使用 row count。 |
| 数组 / payload array | Array / Payload Array | `enter_index`, `enterIndex`, `EnterIndex`, `max_array_length` | Runtime value 或 wire payload 中的物理容器形态。JSON / MessagePack 中 list 通常表现为 array；binary 中 list 表现为 length prefix 后跟 positional elements。`Array` 在 runtime 层只描述 payload shape，不代表 Meta 类型名。 |
| 结构体列表 | Struct List | `list<Struct>` | 元素为 struct 的列表；当前还不是稳定能力。 |
| 结构体依赖图 | Struct Dependency Graph | `StructDependencyGraph` | 用于检测 named struct 循环引用。 |

## Source 与数据读取术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 数据源 | Source | `Source`, `source` | 物理数据来源，例如 CSV、XLSX、Lark Sheet、Google Sheet。 |
| Source Provider | Source Provider | `SourceProvider`, `provider` | 读取 Source 并归一化为二维 Sheet 的加载器。 |
| CSV Source | CSV Source | `CsvSource`, `provider = "csv"` | 本地 CSV 文件 source。 |
| XLSX Source | XLSX Source | `XlsxSource`, `provider = "xlsx"` | 本地 Excel workbook source。 |
| 飞书表格 Source | Lark Sheet Source | `LarkSheetSource`, `provider = "lark_sheet"` | 远程飞书表格 source。 |
| Google Sheet Source | Google Sheet Source | `GoogleSheetSource`, `provider = "google_sheet"` | 远程 Google Sheet source。 |
| Source URI | Source URI | `SourceUri`, `uri` | provider-specific 定位信息。 |
| Source 认证引用 | Source Auth Reference | `SourceAuthRef`, `auth_ref` | 外部凭证引用，不在 Meta 中存放 secret。 |
| Source 范围 | Source Range | `SourceRange`, `range` | 可选表格范围，例如 `A1:K200`。 |
| 二维 Sheet | Source Sheet | `SourceSheet` | provider 输出的统一二维表结构。 |
| Sheet 行 | Sheet Row | `SheetRow` | SourceSheet 中的一行。 |
| Sheet 列 | Sheet Column | `SheetColumn` | SourceSheet 中的一列。 |
| Sheet 单元格 | Sheet Cell | `SheetCell` | 一个 raw cell，包含 row/column/source location。 |
| Source 位置 | Source Location | `SourceLocation` | 源表定位信息，至少包含 source 名称和 row/column 坐标。 |
| 未映射行/列 | Unmapped Row / Column | `UnmappedRow`, `UnmappedColumn` | 没有关联 field path 的空行、空列、注释行或中间数据列，不进入 DataSet。 |
| 表头 | Header | `Header`, `SheetHeader` | 映射 field path 的 header。 |
| 表头映射 | Header Mapping | `HeaderMapping` | 从 header 到 field path 的映射。 |
| Sheet Mapping | Sheet Mapping | `SheetMapping` | table-level source 解释规则。 |
| 复合值书写方式 | Value Layout | `ValueLayout`, `layout` | `scalar_cell`、`json_cell`、`expanded_value`；struct source layout 第一阶段推荐由 CSV/source 中出现 parent path 还是 child paths 自动判定。 |
| 标量单元格 | Scalar Cell | `ValueLayout::ScalarCell`, `scalar_cell` | 简单值写在一个 cell 中。 |
| JSON 单元格 | JSON Cell | `ValueLayout::JsonCell`, `json_cell` | struct/array 等聚合写入一个 JSON cell。 |
| 展开值 | Expanded Value | `ValueLayout::ExpandedValue`, `expanded_value` | 复合值按 field path 展开；物理方向由表类型决定，struct 字段可由 CSV/source path 自动选择该写法。 |
| 独立子表式明细布局 | Detail Table Layout | `DetailTableLayout` | 当前不支持的未来扩展：array 明细放在另一张表中按 key 关联。 |

## 数据模型与视图术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 数据值 | Data Value | `DataValue` | 按 FieldType 解析后的 typed value。 |
| 完整数据集 | Full DataSet | `DataSet` | Source 读取、解析、默认值应用、base merge、override 后的完整逻辑数据。 |
| 表数据 | Table Data | `TableData` | 单张逻辑表的数据。 |
| 对象数据 | Object Data | `ObjectData` | ObjectTable 的 typed object。 |
| 集合数据 | Collection Data | `CollectionData` | CollectionTable 的 records 和 indexes。 |
| 记录 | Record | `Record`, `RecordData` | CollectionTable 中的一条逻辑对象。 |
| 常量对象 | Constant Object | `ObjectData` | ObjectTable 的逻辑输出。 |
| 缺失 | Missing | `Missing` | source 中没有提供字段或 field path。 |
| 空值 | Empty | `Empty` | 提供了空字符串、空数组、空对象等。 |
| Null | Null | JSON `null` | 显式 null 通常出现在 JSON cell 中；当前在解析/校验阶段按 missing/empty 语义处理，不进入 `DataValue`。 |
| 导出视图 | Export View | `ExportView` | 针对 target/context 从 DataSet 派生的过滤后视图。 |
| 导出上下文 | Export Context | `ExportContext` | 一次导出的 target、tags、variables。 |
| 导出 Tag | Export Tag | `ExportTag` | `shipping`、`dev`、`global` 等上下文标签。 |
| 导出过滤 | Export Filter | `ExportFilter` | 按 context 和字段值过滤 record、field 或 fragment。 |
| 源顺序集合 | Source-Ordered Collection | `SourceOrderedCollection` | 保留最终 source composition 顺序的 CollectionTable。 |
| 保留源顺序 | Preserve Source Order | `preserve_source_order` | 显式开启 source order 数组访问。 |

## 索引、外键、多源与 Fragment 术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 索引 | Index | `Index`, `IndexMeta` | CollectionTable 的查询入口和约束定义。 |
| 主索引 | Primary Index | `PrimaryIndex`, `primary_index` | 表的主索引，用于默认外键目标和 CollectionTable override identity。 |
| 唯一索引 | Unique Index | `UniqueIndex`, `unique = true` | 一个 key 至多对应一条 record。 |
| 非唯一索引 | Non-Unique Index | `NonUniqueIndex`, `unique = false` | 一个 key 可以对应多条 record。 |
| 单字段索引 | Single-Field Index | `SingleFieldIndex` | 一个字段构成 key。 |
| 复合索引 | Composite Index | `CompositeIndex` | 多个字段构成 key。 |
| 外键 | Foreign Key | `ForeignKey`, `foreign_key` | 字段值引用另一张表的 unique index 或 primary index。 |
| 外键目标 | Foreign Key Target | `ForeignKeyTarget` | target table 和 target index。 |
| 多数据源 | Multi-Source | `sources`, `override_sources` | 一张逻辑表读取多个物理 source。 |
| 基础 Source | Base Source | `sources` | 通过 split merge 形成基础逻辑表的 source 列表。 |
| 覆盖 Source | Override Source | `override_sources` | 在 base sources 之后按列表顺序覆盖逻辑单元的 source 列表。 |
| 拆分合并 | Split Merge | `SplitMerge` | 多个 base sources 组成一个基础逻辑表。 |
| 覆盖应用 | Override Apply | `OverrideApply` | override source 替换已有逻辑单元。 |
| 行级覆盖 | Row-Level Override | `RowOverride` | CollectionTable 的覆盖粒度：按 primary index 替换整条 record。 |
| 字段级覆盖 | Field-Level Override | `FieldOverride` | ObjectTable 的覆盖粒度：按 field path 覆盖字段。 |
| Source 冲突 | Source Conflict | `SourceConflict` | 多 source 中出现重复、歧义或不允许覆盖的逻辑单元。 |
| 多态记录 | Polymorphic Record | `PolymorphicRecord` | common fields 加零个或多个 fragments 的记录。 |
| 多态集合表 | Polymorphic Collection Table | `PolymorphicCollectionTable` | records 可拥有不同 fragment 集合的 CollectionTable。 |
| 公共字段 | Common Field | `CommonField` | 多态表中所有记录共有字段。 |
| Fragment / 片段 | Fragment | `Fragment`, `FragmentMeta` | 附着到记录上的可组合数据块。 |
| Variant Source | Variant Source | `VariantSource` | 为同一多态表贡献完整记录及指定 fragments 的 source。 |
| Fragment 查询 | Fragment Query | `has_fragment`, `get_fragment` | 运行时代码中检查和获取 fragment 的 API。 |

## 校验与诊断术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 校验 | Validation | `Validation`, `validate` | 导出前的确定性正确性检查。 |
| 校验器 | Validator | `Validator` | 可执行检查单元，输出 diagnostics。 |
| 内置校验器 | Built-In Validator | `BuiltInValidator` | 由 Meta 语义隐含的检查，例如 required、index、foreign key。 |
| 配置校验器 | Configured Validator | `ConfiguredValidator` | 项目显式配置的业务规则检查。 |
| 校验作用域 | Validation Scope | `ValidationScope` | Project、Source、Field、Record、Collection、Multi-Collection、Export。 |
| 字段级校验 | Field Validation | `FieldValidator` | 检查字段值。 |
| 记录级校验 | Record Validation | `RecordValidator` | 检查一条 record 内部关系。 |
| 集合级校验 | Collection Validation | `CollectionValidator` | 检查当前验证视图中的集合。 |
| 多集合校验 | Multi-Collection Validation | `MultiCollectionValidator` | 跨表检查，例如 foreign key。 |
| 导出级校验 | Export Validation | `ExportValidator` | 检查 target mapping、artifact 路径等。 |
| 校验视图 | Validation View | `ValidationView` | 当前校验使用的 DataSet 或 ExportView。 |
| 诊断 | Diagnostic | `Diagnostic` | 面向用户和机器的错误/警告/信息。 |
| 诊断码 | Diagnostic Code | `DiagnosticCode` | 稳定机器可读错误码，例如 `field.required_missing`。 |
| 诊断严重级别 | Diagnostic Severity | `DiagnosticSeverity` | `error`、`warning`、`info`。 |
| 诊断收集器 | Diagnostic Collector | `DiagnosticCollector` | 收集各阶段 diagnostics 并稳定合并。 |
| 诊断报告器 | Diagnostic Reporter | `DiagnosticReporter` | 输出 human、json、jsonl、summary 格式。 |
| 相关位置 | Related Location | `RelatedLocation` | 多位置错误中的关联 source location。 |

## 导出、CLI 与产物术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 导出 | Export | `Export`, `export` | 生成运行时数据、代码和特殊产物。 |
| 导出目标 | Export Target | `ExportTarget` | 一个配置化输出目标。 |
| 导出任务实例 | Export Task Instance | `NormalizedExportTask`, `task.name` | `[[targets.exports]]` 中的一条任务声明；`name` 是同一 target 内的实例名，同一个 generator type 可以用不同 name / output / options 声明多次。 |
| 生成器类型标识 | Generator Type ID | `type_id` | build-facing generator 的全局类型标识，用于匹配 `[[targets.exports]].type`。 |
| 生成器别名 | Generator Alias | `aliases` | generator type id 的兼容别名；registry 匹配时与 type id 一起组成 generator identity。 |
| 生成器描述符 | Generator Descriptor | `GeneratorDescriptor` | 聚合 generator type id、aliases 和 requirements 的调度侧描述；registry 通过 descriptor 做匹配并读取 requirements。 |
| Artifact 生成器 | Artifact Generator | `ArtifactGenerator` | 根据声明的 input requirements 生成一个或多个 artifact 的 build-facing 抽象；requirements 决定调度，code/data/doc/report 等能力通过 facet/helper 组合。 |
| 生成器输入需求 | Generator Requirements | `GeneratorRequirements` | 描述 generator 需要哪些输入，例如 Meta、Data、assets、validation report；当前 branch pipeline 直接用 `needs_data` 拆分 MetaOnly / MetaAndData 执行分组。 |
| 生成器能力 Facet | Generator Facet | `CodeArtifactOptions`, `DocArtifactFacet` | 可组合的领域能力或选项集合，例如代码文本选项、runtime table serialization、聚合 artifact、文档渲染。 |
| Code Artifact Toolkit | Code Artifact Toolkit | `code_artifact` | 供 code-like generator 复用的工具箱，包括 CodeModel IR、CodeWriter、naming / formatting / keyword conflict、`CodeArtifactOptions` 和 runtime reader contract helper。物理路径为 `src/code_artifact/`；不再作为具体语言 generator 的归属地。 |
| Data Artifact Toolkit | Data Artifact Toolkit | `data_artifact` | 供 data-backed generator 复用的工具箱，包括 target-aware payload、`ExportedTablePayload` / `ExportedValue`、runtime table serialization、binary layout 和 legacy data exporter adapter。物理路径为 `src/data_artifact/`；不再作为具体 export type generator 的归属地。 |
| 数据导出器 | Data Exporter | `DataExporter` | 从 ExportView 写 runtime data 的迁移期 helper / adapter；目标态具体 JSON / MessagePack / `bin` 等导出类型应作为 artifact generator 注册。 |
| 运行时数据格式 | Runtime Data Format | `RuntimeDataFormat` | 当前 runtime table data export type 支持 `json`、`messagepack`、`bin`；当前内置聚合 / 内嵌数据 export type 支持 `gettext_pot` / `pot`、`lua_table` / `lua`；未来可扩展 `yaml`、`sqlite`、`protobuf`。 |
| JSON 导出 | JSON Export | `JsonDataExporter` | 输出 JSON runtime data。 |
| Lua Table 导出 | Lua Table Export | `LuaTableExporter` | 输出 Lua table；Lua 中代码和数据可合一。 |
| Lua 内嵌数据生成器 | Lua Embedded Data Generator | `LuaTableGenerator` / `LuaEmbeddedDataExporter` | `type = "lua_table"` 的 data-backed mixed generator；requirements 包含 Meta + Data，输出 code-like Lua artifact，生成 root facade 与每表 `.lua` 文件。 |
| YAML 导出 | YAML Export | `YamlDataExporter` | 可扩展 runtime data exporter。 |
| MessagePack 导出 | MessagePack Export | `MessagePackDataExporter` | 第一阶段支持的紧凑二进制 runtime data exporter，配置规范名为 `messagepack`。 |
| SQLite 导出 | SQLite Export | `SqliteDataExporter` | 可扩展数据库 exporter。 |
| Protobuf 导出 | Protobuf Export | `ProtobufDataExporter` | 未来扩展；需要显式 `field_number`。 |
| 聚合数据导出器 | Aggregate Data Exporter | `AggregateDataExporter` | 输出跨表辅助数据产物的迁移期 data exporter helper，例如 POT、asset manifest；目标态具体聚合产物应作为 data-backed artifact generator 注册。 |
| POT 导出器 | Gettext POT Exporter | `GettextPotExporter` | `type = "gettext_pot"`；从 ExportView 中的 `text` 字段导出 POT。 |
| 翻译注释 | Translator Comment | `TranslatorComment` | POT 中给翻译人员的注释，可来自 text 字段、`desc` 或 source 中额外映射的注释列/行。 |
| 资源清单导出器 | Asset Manifest Exporter | `AssetManifestExporter` | 从 ExportView 中的 `asset` 字段导出资源清单。 |
| 代码生成器 | Code Generator | `CodeGenerator` | 生成目标语言代码或访问 API 的迁移期 code artifact 能力接口。当前内置 language codegen 只依赖 Meta；但 “是否需要 Data” 应由 generator requirements 声明，不应作为 CodeGenerator 继承层级的一部分。目标态具体语言生成器应作为 `src/generator/<language>/` 下的 artifact generator，`CodeGenerator` 可删除或降级为 toolkit helper。 |
| 类型映射 | Type Mapping | `TypeMapping` | 逻辑类型到目标语言/引擎类型的映射。 |
| 目标类型 | Target Type | `TargetTypeName` | 目标语言或引擎中的具体类型名。 |
| 导出产物 | Export Artifact | `ExportArtifact` | 一个生成文件，例如 runtime data、code、POT、asset manifest。 |
| 产物清单 | Artifact Manifest | `ArtifactManifest` | 可由聚合 data exporter 生成的本次导出产物列表。 |
| 产物写入器 | Artifact Writer | `ArtifactWriter` | 统一写文件、dry-run 和 temp-commit 的组件。 |
| 产物写入策略 | Artifact Write Mode | `ArtifactWriteMode` | `dry-run`、`direct` 或 `temp-commit`。 |
| 临时提交写入 | Temp Commit Write | `ArtifactWriteMode::TempCommit`, `temp-commit` | 先写临时文件，成功后统一提交到最终路径。 |
| 直接写入 | Direct Write | `ArtifactWriteMode::Direct`, `direct` | 直接写最终路径。 |
| CLI 命令 | CLI Command | `CliCommand` | `validate`、`export`、`inspect`、`debug`。 |
| 校验命令 | Validate Command | `validate` | 只读取、组合、过滤和校验，不写正式产物。 |
| 导出命令 | Export Command | `export` | 校验通过后生成产物。 |
| 检查命令 | Inspect Command | `inspect` | 查看合并后的 MetaModel、source、target、desc 等。 |
| 调试命令 | Debug Command | `debug` | 输出插件、export plan、diagnostics schema 等排障信息。 |

## 插件与执行基础设施术语

| 中文名词 | Recommended English | Recommended Code Symbol | 含义 / 说明 |
| --- | --- | --- | --- |
| 插件 | Plugin | `Plugin` | 内置或动态加载的能力扩展。 |
| 动态库插件 | Dynamic Library Plugin | `DynamicPlugin`, `PluginLibrary` | `.dylib`、`.so`、`.dll` 等动态库插件。 |
| 插件清单 | Plugin Manifest | `PluginManifest` | 插件 ID、版本、API 版本和能力声明。 |
| 插件能力 | Plugin Capability | `PluginCapability` | SourceProvider、DataExporter、CodeGenerator、Validator、BuiltinType 等能力。 |
| Generator Registry | Generator Registry | `GeneratorRegistry` | build-facing artifact generator 注册表 / facade；根据 export type 查询 generator requirements，并在迁移期适配 data exporter registry、code generator registry 和 direct artifact generator。目标态直接注册 `src/generator/<type_id>/` 下的 concrete generator。 |
| Source Provider Registry | Source Provider Registry | `SourceProviderRegistry` | provider ID 到 provider factory 的注册表。 |
| Data Exporter Registry | Data Exporter Registry | `DataExporterRegistry` | data export format 到 table/aggregate data exporter 的迁移期领域注册表；由 `GeneratorRegistry` adapter 暴露给 build-facing 调度，目标态删除或降级为 data artifact toolkit 内部 helper。 |
| Code Generator Registry | Code Generator Registry | `CodeGeneratorRegistry` | language/target 到 code generator 的迁移期领域注册表；只表达 code artifact 能力，不作为所有 MetaOnly generator 的总基类，目标态删除或降级为 code artifact toolkit 内部 helper。 |
| Validator Registry | Validator Registry | `ValidatorRegistry` | validator kind 到 validator factory 的注册表。 |
| Builtin Type Registry | Builtin Type Registry | `BuiltinTypeRegistry` | builtin value type 到 parser/validator 的注册表。 |
| 任务执行器 | Task Executor | `TaskExecutor` | 执行校验和导出任务，可并行。 |
| 执行计划 | Execution Plan | `ExecutionPlan` | 并行执行前解析出的确定性任务计划。 |
| 导出计划 | Export Plan | `ExportPlan` | artifact 生成前的计划，包含路径冲突检查。 |
| 异步 Source Provider | Async Source Provider | `AsyncSourceProvider` | 可进行网络或文件 I/O 的 source provider API。 |
| 计算任务 | CPU-Bound Task | `CpuBoundTask` | 应使用线程池调度的解析、校验或生成任务。 |
| I/O 任务 | I/O-Bound Task | `IoBoundTask` | 应优先使用异步接口或操作系统 I/O 机制调度的文件/网络任务。 |

## 不再推荐或已取消的旧术语

| 旧术语 | 当前处理 |
| --- | --- |
| `ExportDataset` | 改用 `ExportView`。 |
| `primary_key` / Primary Key | 取消；改用 `primary_index` 或 index 上 `primary = true`。 |
| `local_file` provider | 取消；本地 CSV/XLSX 分别使用 `provider = "csv"` / `provider = "xlsx"`。 |
| Source `priority` | 取消；`override_sources` 按列表顺序覆盖，后者覆盖前者。 |
| `expanded_columns` / `expanded_rows` | 不作为 canonical layout；改用 `expanded_value`。 |
| `color_channel = "auto"` | 禁止；必须显式为 `byte` 或 `normalized`。 |
| 模糊 `custom` runtime format | 不使用；自定义格式必须注册明确 export type / exporter ID。当前 runtime table data export type 为 `json`、`messagepack`、`bin`；聚合 exporter 已支持 `gettext_pot` / `pot`、`lua_table` / `lua`。 |
| fragment join mode | 当前不支持；当前只支持 variant source mode。 |
