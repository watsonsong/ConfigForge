# Glossary

This document records ConfigForge terminology, Chinese/English naming, and recommended code symbols. Use it as the naming source of truth when writing feature specs, Meta design, and future Rust code.

## Naming Principles

- Prefer precise domain terms over generic names.
- Use `Meta` for schema and definition concepts, not `Config`, when referring to export definitions.
- Use `ObjectTable` for constant-object-style tables.
- Use `CollectionTable` for row-collection-style tables.
- Use `TableKind` as the discriminator between supported table forms.
- Treat `Field` as the basic Meta unit shared by both object tables and collection tables.
- Treat `FieldType` as logical value shape, and `FieldLayout` / `SheetMapping` as physical sheet mapping.
- Use `string` for plain non-localized text and `text` for i18n-aware localized text.
- Use `asset` for engine resource references that require export-time existence validation.
- Use builtin value types such as `color_rgba`, `vector3`, `rotation`, and `datetime` for common game-development composite values that should be authorable in one cell.
- Use `enum` for finite domain values declared in Meta and validated before export.
- Use `NamedStruct` for reusable struct definitions that can be referenced by fields, fragments, or other structs.
- Keep target-language and engine-specific type names in export/codegen configuration rather than in logical Meta definitions.
- Use `Validator` for executable correctness checks and `ValidationScope` for where a check applies.
- Use `SourceSheet` for normalized two-dimensional source data and `DataSet` for typed in-memory logical data.
- Use `ExportDataset` for the target/context-specific data view passed to data exporters.
- Use `RuntimeDataFormat` for generated runtime file formats such as JSON, YAML, MessagePack, Protobuf, and SQLite.
- Use `Plugin` for dynamically loaded or built-in capability extensions, and route all extension discovery through registries.
- Use async APIs for Source Providers and Artifact Writers that may touch network or filesystem I/O.
- Use deterministic execution plans and diagnostic ordering even when implementation uses parallel tasks.
- Use `DiagnosticCollector` and structured diagnostic reports for debuggable batch error output.
- Keep Rust type names in `PascalCase`, functions and fields in `snake_case`, and file/module names in lowercase `snake_case`.

## Terms

| 中文名词 | Recommended English | Recommended Code Symbol | Meaning / Notes |
| --- | --- | --- | --- |
| 元数据 / 元定义 | Meta | `Meta`, `meta` | Defines tables, fields, field types, indexes, constraints, and export-facing structure. |
| Meta 文件 | Meta File | `MetaSet`, `TableMeta` | File or file set that stores Meta definitions. |
| 内存 Meta 模型 | In-Memory Meta Model | `MetaModel`, `MetaSet` | Parsed and validated object representation used by ConfigForge internally. |
| Meta 文档 | Meta Document | `MetaDocument` | Physical file representation of Meta definitions, such as TOML or XML. |
| Meta 语法 | Meta Syntax | `MetaSyntax` | File syntax used to author Meta documents. Initial targets are TOML and XML. |
| TOML Meta | TOML Meta | `TomlMetaReader` | Human-friendly Meta syntax, suitable for simple hand-authored definitions. |
| XML Meta | XML Meta | `XmlMetaReader` | Structured Meta syntax intended to work with XML Schema and IDE validation. |
| XML Schema | XML Schema / XSD | `MetaSchema`, `XmlSchema` | Schema file used by XML-aware IDEs to validate Meta document structure. |
| 配置表 | Table / Data Table | `Table`, `TableMeta` | Designer-authored table described by Meta and exported for game usage. |
| 表类型 | Table Kind | `TableKind`, `table_kind` | Discriminator between object tables and collection tables. |
| 对象表 / 列表 | Object Table | `ObjectTable`, `TableKind::Object` | Each source row describes one field; the whole table represents one constant object. |
| 集合表 / 行表 | Collection Table | `CollectionTable`, `TableKind::Collection` | Each source row describes one object; each source column describes one field. |
| 多态记录 | Polymorphic Record | `PolymorphicRecord` | One logical record that may own different fragments while sharing common fields and one ID namespace. |
| 多态集合表 | Polymorphic Collection Table | `PolymorphicCollectionTable` | A collection table whose records may have different fragment sets. |
| 公共字段 | Common Field | `CommonField` | Field shared by every variant/source of a polymorphic table, such as name or icon. |
| Fragment / 片段 | Fragment | `Fragment`, `FragmentMeta` | Optional composable data block attached to a record, preferred over inheritance for variant-specific data. |
| Fragment 类型 | Fragment Type | `FragmentType`, `fragment_type` | Stable identifier for a fragment kind, such as weapon, equipment, stats, skill, or usable. |
| Fragment 表 | Fragment Source Table | `FragmentSource`, `FragmentTable` | Physical data sheet for one variant or fragment-bearing subset of a polymorphic table. |
| Fragment 查询 | Fragment Query | `has_fragment`, `get_fragment` | Generated runtime query that checks whether a record has a fragment and retrieves it when present. |
| 字段 | Field | `Field`, `FieldMeta` | A named value defined by Meta. |
| 基础字段单位 | Field as Table Unit | `Field`, `FieldMeta` | ObjectTable and CollectionTable are both composed from fields. |
| 字段名 | Field Name | `field_name`, `name` | Stable identifier for a field. |
| 字段类型 | Field Type | `FieldType`, `field_type` | Primitive or future composite value type. |
| 基础类型 | Primitive Field Type | `PrimitiveFieldType` | Scalar types such as int, float, bool, string, text, and asset. |
| 内置值类型 | Builtin Value Type | `BuiltinValueType` | Common ConfigForge-provided composite type that does not require a user-defined struct. |
| 整数 | Integer | `FieldType::Int`, `int` | Integer numeric value. |
| 浮点数 | Float | `FieldType::Float`, `float` | Floating-point numeric value. |
| 普通文本 | String | `FieldType::String`, `string` | Plain non-localized text. |
| 国际化文本 | Text / Localized Text | `FieldType::Text`, `LocalizedText`, `text` | Text that participates in localization/i18n export workflows. |
| 资源引用 | Asset Reference | `FieldType::Asset`, `AssetRef`, `asset` | Reference to an engine resource; export validation should check existence. |
| RGB 颜色 | Color RGB | `BuiltinValueType::ColorRgb`, `color_rgb`, `ColorRgb` | Three-channel logical color, commonly authored as hex or rgb tuple. |
| RGBA 颜色 | Color RGBA | `BuiltinValueType::ColorRgba`, `color_rgba`, `ColorRgba` | Four-channel logical color with alpha, commonly authored as hex or rgba tuple. |
| 颜色通道策略 | Color Channel Policy | `ColorChannelPolicy`, `color_channel` | Channel interpretation such as byte, normalized, or auto. |
| 颜色空间 | Color Space | `ColorSpace`, `color_space` | Logical color-space metadata such as sRGB or linear for exporters. |
| 二维向量 | Vector2 | `BuiltinValueType::Vector2`, `vector2`, `Vector2` | Two-component float vector. |
| 三维向量 | Vector3 | `BuiltinValueType::Vector3`, `vector3`, `Vector3` | Three-component float vector. |
| 四维向量 | Vector4 | `BuiltinValueType::Vector4`, `vector4`, `Vector4` | Four-component float vector. |
| 整数二维向量 | Vector2Int | `BuiltinValueType::Vector2Int`, `vector2_int`, `Vector2Int` | Two-component integer vector, useful for grid coordinates. |
| 整数三维向量 | Vector3Int | `BuiltinValueType::Vector3Int`, `vector3_int`, `Vector3Int` | Three-component integer vector, useful for voxel/grid coordinates. |
| 欧拉旋转 | Rotation / Euler Rotation | `BuiltinValueType::Rotation`, `rotation`, `Rotation` | Three-component Euler rotation with configurable unit/order. |
| 旋转单位 | Rotation Unit | `RotationUnit`, `rotation_unit` | Angle unit such as degrees or radians. |
| 旋转顺序 | Rotation Order | `RotationOrder`, `rotation_order` | Euler order such as xyz or zyx. |
| 日期时间 | DateTime | `BuiltinValueType::DateTime`, `datetime`, `DateTimeValue` | Date-time value with optional pattern and timezone policy. |
| 日期 | Date | `BuiltinValueType::Date`, `date`, `DateValue` | Calendar date without time-of-day. |
| 时间 | Time | `BuiltinValueType::Time`, `time`, `TimeValue` | Time-of-day without date. |
| 持续时间 | Duration | `BuiltinValueType::Duration`, `duration`, `DurationValue` | Time span such as seconds, minutes, hours, or days. |
| 时间格式 | DateTime Pattern | `DateTimePattern`, `datetime_pattern` | Field-level pattern used to parse datetime/date/time strings. |
| 时区 | Time Zone | `TimeZone`, `timezone` | Timezone policy for datetime values without explicit zone. |
| 整数区间 | Integer Range | `BuiltinValueType::IntRange`, `int_range`, `IntRange` | Inclusive integer range, useful for levels or random integer rolls. |
| 浮点区间 | Float Range | `BuiltinValueType::FloatRange`, `float_range`, `FloatRange` | Inclusive float range, useful for random scalar rolls. |
| 矩形 | Rect | `BuiltinValueType::Rect`, `rect`, `Rect` | 2D rectangle, useful for UI or screen regions. |
| 三维包围盒 | Bounds3 | `BuiltinValueType::Bounds3`, `bounds3`, `Bounds3` | 3D bounds, useful for spatial volumes; exact authoring convention may be target/project specific. |
| 结构体 | Struct | `FieldType::Struct`, `StructMeta` | Composite value packed from multiple child fields. |
| 匿名结构体 | Inline Struct / Anonymous Struct | `InlineStruct`, `StructMeta` | Struct shape declared directly inside one field definition. |
| 命名结构体 / 可复用结构体 | Named Struct / Reusable Struct | `NamedStruct`, `StructDef`, `StructRef` | Reusable struct definition declared once and referenced by fields or other structs. |
| 结构体引用 | Struct Reference | `FieldType::StructRef`, `StructRef` | Field type that references a named struct by stable name instead of redefining its fields inline. |
| 结构体依赖图 | Struct Dependency Graph | `StructDependencyGraph` | Directed graph where named structs point to other named structs they reference. Used for cycle detection. |
| 结构体循环引用 | Struct Cycle | `StructCycle`, `CycleDetection` | Invalid recursive struct reference chain such as `A -> B -> A`, unless a future indirection policy explicitly supports it. |
| 数组 / 集合字段 | Array | `FieldType::Array`, `ArrayMeta` | Repeated values where each element has its own field type. |
| 结构体数组 | Struct Array | `Array<Struct>`, `StructArrayField` | Array whose element type is a struct. |
| 枚举 | Enum | `EnumDef`, `FieldType::Enum` | Finite set of allowed symbolic values declared in Meta and validated before export. |
| 枚举项 | Enum Variant / Enum Item | `EnumVariant`, `EnumItem` | One allowed value in an enum, usually with a stable name and optional numeric/code value. |
| 枚举引用 | Enum Reference | `EnumRef` | Field type reference to a named enum definition. |
| 嵌套字段 | Nested Field | `NestedField` | Field contained by a struct or array element. |
| 字段路径 | Field Path | `FieldPath`, `field_path` | Dot/bracket path such as `reward.item_id` or `rewards[0].count` used for mapping and diagnostics. |
| 行 | Row | `Row`, `row` | One physical row from a source table. Its logical meaning depends on table kind. |
| 列 | Column | `Column`, `column` | One physical column from a source table. |
| 单元格 | Cell | `Cell`, `cell` | One raw value at a row/column intersection. |
| 二维 Sheet | Source Sheet / Raw Sheet | `SourceSheet`, `RawSheet` | Normalized two-dimensional table read from a source provider, addressable by row and column. |
| Sheet 行 | Sheet Row | `SheetRow` | One row in a normalized source sheet. |
| Sheet 单元格 | Sheet Cell | `SheetCell` | One raw cell with row/column coordinates and source location. |
| 表头 | Sheet Header | `SheetHeader` | First sheet row used to map physical columns to Meta fields or field paths. |
| 表头映射 | Header Mapping | `HeaderMapping`, `header_mapping` | Mapping from sheet headers to logical fields or field paths. |
| Sheet 映射 | Sheet Mapping | `SheetMapping`, `sheet_mapping` | Table-level rule for interpreting physical sheet layout. |
| 字段布局 | Field Layout | `FieldLayout`, `layout` | Field-level rule for reading one value from source cells. |
| JSON 单元格 | JSON Cell | `JsonCell`, `LayoutKind::JsonCell` | A single cell containing JSON for a struct, array, or nested value. |
| 展开布局 | Expanded Layout | `ExpandedLayout`, `LayoutKind::ExpandedColumns`, `LayoutKind::ExpandedRows` | Struct or array values expanded across rows or columns. |
| 记录 / 数据对象 | Record | `Record`, `record` | One logical object parsed from a collection table row. |
| 常量对象 | Constant Object | `ConstantObject`, `object` | The logical result produced by an object table. |
| 数据集合 | Collection | `Collection`, `records` | The logical result produced by a collection table. |
| 数据值 | Data Value | `DataValue` | Typed in-memory value parsed according to Meta, such as int, enum, struct, or array. |
| 数据集 | Data Set | `DataSet` | Complete typed in-memory logical data after source parsing/composition and before target-specific filtering. |
| 表数据 | Table Data | `TableData` | Typed data for one logical table, either object or collection. |
| 对象数据 | Object Data | `ObjectData` | Typed data for one ObjectTable. |
| 集合数据 | Collection Data | `CollectionData` | Typed records and indexes for one CollectionTable. |
| 集合访问策略 | Collection Access Strategy | `CollectionAccessStrategy` | How a CollectionTable is exposed at runtime: indexed lookup, source-ordered array, or both. |
| 源顺序集合 | Source-Ordered Collection | `SourceOrderedCollection` | CollectionTable whose records are exported as an array preserving final source composition order. |
| 保留源顺序 | Preserve Source Order | `preserve_source_order` | Explicit table-level opt-in for ordered array access without requiring an index. |
| 顺序数组导出 | Ordered Array Export | `OrderedArrayExport` | Runtime export view where collection records are emitted as an array in preserved order. |
| 导出数据视图 | Export Dataset | `ExportDataset` | Target/context-specific view derived from `DataSet` after filters and runtime field selection. |
| 索引 | Index | `Index`, `IndexMeta` | Lookup-oriented field or field group that drives validation and generated query code. |
| 主索引 | Primary Index | `PrimaryIndex`, `primary_index` | Table-level alias to one declared unique index; used by foreign keys when no target index name is specified. |
| 主键 | Primary Key | `PrimaryKey`, `primary_key` | Not a canonical Meta concept. If supported later, it should only be compatibility shorthand for a single-field unique primary index. |
| 唯一索引 | Unique Index | `UniqueIndex`, `unique` | Index where one key maps to at most one object. |
| 非唯一索引 | Non-Unique Index / Multi-Value Index | `MultiIndex`, `multi_index` | Index where one key may map to multiple objects. |
| 单字段索引 | Single-Field Index | `SingleFieldIndex` | Index key built from one field. |
| 复合索引 | Composite Index | `CompositeIndex` | Index key built from multiple fields. |
| 无序索引 | Unordered Index | `UnorderedIndex`, `IndexOrder::Unordered` | Index optimized for exact lookup without ordering semantics. |
| 有序索引 | Ordered Index | `OrderedIndex`, `IndexOrder::Ordered` | Index that preserves key order and can support range queries. |
| 范围查询 | Range Query | `RangeQuery` | Query over an ordered index using lower/upper bounds. |
| 密集整数索引 | Dense Integer Index | `DenseIntIndex`, `IndexStorage::DenseArray` | Integer single-field index whose keys must be contiguous with no holes. |
| 索引存储 | Index Storage | `IndexStorage` | Runtime storage strategy, such as hash table, sorted array, or dense array. |
| 外键 | Foreign Key | `ForeignKey`, `foreign_key` | Field-level reference to another table through a unique target index. |
| 外键目标 | Foreign Key Target | `ForeignKeyTarget` | Target table and optional target index for a foreign key. |
| 约束 | Constraint | `Constraint`, `constraint` | Validation rule attached to a table, index, or field. |
| 唯一约束 | Unique Constraint | `UniqueConstraint`, `unique` | Ensures a field or index value is unique within its validation scope. |
| 校验 | Validation | `Validation`, `validate` | Deterministic checks performed before export. |
| 校验器 | Validator | `Validator` | Executable correctness check that emits diagnostics. |
| 内置校验器 | Built-In Validator | `BuiltInValidator` | Validator implied by Meta semantics, such as required, unique index, foreign key, or struct cycle checks. |
| 配置校验器 | Configured Validator | `ConfiguredValidator` | Validator explicitly declared in Meta or validation configuration for project-specific rules. |
| 校验作用域 | Validation Scope | `ValidationScope` | Scope where a validator runs: field, record, collection, multi-collection, project, or export. |
| 字段级校验器 | Field Validator | `FieldValidator` | Validator that checks one field value or field path. |
| 记录级校验器 | Record Validator | `RecordValidator` | Validator that checks one object/record and may compare multiple fields. |
| 集合级校验器 | Collection Validator | `CollectionValidator` | Validator that checks one final logical table or collection. |
| 多集合校验器 | Multi-Collection Validator | `MultiCollectionValidator` | Validator that checks relationships across multiple tables. |
| 工程级校验器 | Project Validator | `ProjectValidator` | Validator that checks project-level Meta, source, or configuration correctness. |
| 导出级校验器 | Export Validator | `ExportValidator` | Validator that checks export target and generated artifact correctness. |
| 校验阶段 | Validation Phase | `ValidationPhase` | Deterministic stage such as meta, source normalization, field, record, collection, or export. |
| 校验上下文 | Validation Context | `ValidationContext` | Read-only inputs needed by validators, including Meta, typed data, indexes, and source locations. |
| 校验诊断 | Validation Diagnostic | `ValidationDiagnostic` | Structured error/warning/info produced by a validator. |
| 校验注册表 | Validator Registry | `ValidatorRegistry` | Registry of built-in and future custom validators by kind/scope. |
| 校验计划 | Validation Plan | `ValidationPlan` | Resolved deterministic execution plan for validators. |
| 错误码 | Diagnostic Code | `DiagnosticCode` | Stable machine-readable validation code such as `field.required_missing`. |
| 导出 | Export | `Export`, `export` | Generate runtime data and code artifacts. |
| 数据导出器 | Data Exporter | `DataExporter` | Exporter that writes runtime data files from an `ExportDataset`. |
| 运行时数据格式 | Runtime Data Format | `RuntimeDataFormat` | Output data file format such as JSON, YAML, MessagePack, Protobuf, SQLite, or custom binary. |
| JSON 数据导出 | JSON Data Export | `JsonDataExporter` | Runtime data exporter that writes JSON artifacts. |
| YAML 数据导出 | YAML Data Export | `YamlDataExporter` | Runtime data exporter that writes YAML artifacts. |
| MessagePack 数据导出 | MessagePack Data Export | `MessagePackDataExporter` | Runtime data exporter that writes compact binary JSON-like artifacts. |
| Protobuf 数据导出 | Protobuf Data Export | `ProtobufDataExporter` | Runtime data exporter that writes schema and binary data artifacts. |
| SQLite 数据导出 | SQLite Data Export | `SqliteDataExporter` | Runtime data exporter that writes SQLite database files. |
| 导出产物 | Export Artifact | `ExportArtifact` | One generated file, such as runtime data, schema, generated code, POT, or asset manifest. |
| 产物清单 | Artifact Manifest | `ArtifactManifest` | List of generated artifacts for build tooling, CI, and packaging. |
| 特殊导出器 | Special Exporter | `SpecialExporter` | Exporter that creates auxiliary artifacts instead of regular runtime data. |
| POT 文本导出 | Gettext POT Exporter | `GettextPotExporter` | Special exporter that extracts `text` fields into gettext POT templates. |
| 资源清单导出 | Asset Manifest Exporter | `AssetManifestExporter` | Special exporter that extracts `asset` references into a packaging manifest such as CSV. |
| 导出上下文 | Export Context | `ExportContext` | Context for one export run, including target, tags, and variables. |
| 导出 Tag | Export Tag | `ExportTag` | Context tag such as `dev`, `test`, `shipping`, `cn`, or `global`. |
| 导出过滤 | Export Filter | `ExportFilter` | Rule that includes/excludes records, fields, fragments, or special entries according to context and values. |
| 运行时字段 | Runtime Field | `RuntimeField` | Field emitted to regular runtime data. |
| 工具字段 | Tool-Only Field | `ToolOnlyField`, `tool_only` | Field parsed and validated by tools but omitted from regular runtime data by default. |
| 过滤字段 | Filter Field | `FilterField`, `filter_field` | Field allowed to participate in export filter predicates. |
| 仅校验字段 | Validator-Only Field | `ValidatorOnlyField`, `validator_only` | Field used by validators but omitted from runtime output. |
| 代码导出 | Code Export / Code Generation | `CodeExport`, `CodeGenerator` | Generate target-language code stubs or runtime access APIs from validated Meta and data. |
| 导出配置 | Export Configuration | `ExportConfig`, `ExportTarget` | Target-specific configuration for generated data/code, separate from logical Meta definitions. |
| 导出目标 | Export Target | `ExportTarget` | One generated artifact target, such as TypeScript for Unreal tooling or Lua for Unity runtime. |
| 类型映射 | Type Mapping | `TypeMapping`, `TypeMapper` | Rule that maps logical Meta types, enums, and named structs to target-language or engine-specific types. |
| 逻辑类型 | Logical Type | `LogicalType`, `FieldType` | Meta-owned type shape such as `int`, `color_rgba`, `enum<Rarity>`, or `struct<Reward>`. |
| 目标类型 | Target Type | `TargetType`, `TargetTypeName` | Export/codegen-owned type spelling such as `FLinearColor`, `Color`, `number`, or `string`. |
| 目标类型别名 | Target Type Alias | `TargetTypeAlias` | Export mapping entry that maps one logical type to a custom target type name. |
| 运行时表示 | Runtime Representation | `RuntimeRepresentation` | Target-specific data/code representation chosen by an exporter, not by Meta. |
| 插件 | Plugin | `Plugin` | Built-in or dynamically loaded capability extension. |
| 动态库插件 | Dynamic Library Plugin | `DynamicPlugin`, `PluginLibrary` | Plugin loaded from `.dylib`, `.so`, or `.dll`. |
| 插件清单 | Plugin Manifest | `PluginManifest` | Plugin-declared ID, version, API version, and capabilities. |
| 插件能力 | Plugin Capability | `PluginCapability` | Capability registered by a plugin, such as source provider, exporter, validator, or builtin type. |
| 插件注册表 | Plugin Registry | `PluginRegistry` | Registry that owns plugin discovery and capability registration. |
| Source Provider 插件 | Source Provider Plugin | `SourceProviderPlugin` | Plugin capability that registers one or more source providers. |
| 数据导出插件 | Data Exporter Plugin | `DataExporterPlugin` | Plugin capability that registers runtime data exporters. |
| 代码生成插件 | Code Generator Plugin | `CodeGeneratorPlugin` | Plugin capability that registers target-language code generators. |
| Validator 插件 | Validator Plugin | `ValidatorPlugin` | Plugin capability that registers validator factories. |
| 内置类型插件 | Builtin Type Plugin | `BuiltinTypePlugin` | Plugin capability that registers builtin value type parsing and validation. |
| 特殊导出插件 | Special Exporter Plugin | `SpecialExporterPlugin` | Plugin capability that registers auxiliary exporters such as POT or asset manifests. |
| 异步 Source Provider | Async Source Provider | `AsyncSourceProvider` | Source provider API that can load local or network sources asynchronously. |
| 异步产物写入器 | Async Artifact Writer | `AsyncArtifactWriter`, `ArtifactWriter` | Async interface for writing generated artifacts and committing manifests. |
| 任务执行器 | Task Executor | `TaskExecutor` | Runtime component that executes validation/export tasks, possibly in parallel. |
| 并行执行计划 | Parallel Execution Plan | `ParallelExecutionPlan`, `ValidationExecutionPlan` | Deterministic plan whose independent tasks may run concurrently. |
| 诊断收集器 | Diagnostic Collector | `DiagnosticCollector` | Thread-local or shared component that collects diagnostics before deterministic merge. |
| 诊断报告器 | Diagnostic Reporter | `DiagnosticReporter` | Formats diagnostics as human output, JSON, JSON Lines, or summaries. |
| 结构化诊断报告 | Structured Diagnostic Report | `DiagnosticReport` | Machine-readable diagnostic output for CI, IDE, or tooling integration. |
| 相关位置 | Related Location | `RelatedLocation` | Additional source location attached to a diagnostic, useful for duplicate definitions or conflicts. |
| 数据源 | Source | `Source`, `source` | CSV now; future spreadsheet providers can be normalized into the same model. |
| Source Provider | Source Provider | `SourceProvider`, `provider` | Backend responsible for loading one source, such as local CSV, XLSX, Lark Sheet, or Google Sheet. |
| Source URI | Source URI | `SourceUri`, `uri` | Provider-specific locator for a source, such as a file path, URL, document ID, sheet ID, or range. |
| 本地文件 Source | Local File Source | `LocalFileSource` | Local filesystem source provider for CSV, XLSX, or other file formats. |
| CSV Source | CSV Source | `CsvSource` | Local or remote comma-separated sheet source. |
| XLSX Source | XLSX Source | `XlsxSource` | Excel workbook source, usually requiring workbook path plus sheet name. |
| 飞书 Sheet Source | Lark Sheet Source / Feishu Sheet Source | `LarkSheetSource` | Remote spreadsheet provider backed by Lark/Feishu Sheets. |
| Google Sheet Source | Google Sheet Source | `GoogleSheetSource` | Remote spreadsheet provider backed by Google Sheets. |
| Source 认证引用 | Source Auth Reference | `SourceAuthRef`, `auth_ref` | Name or reference to credentials managed outside Meta files. |
| Source 范围 | Source Range | `SourceRange`, `range` | Optional sheet range selector such as `A1:K200`. |
| 多数据源 | Multi-Source | `MultiSource`, `sources`, `override_sources` | One table reading from multiple physical source files or sheets. |
| 基础 Source | Base Source | `sources` | Physical inputs that are split/combined into the base logical table and must not conflict on unique indexes. |
| 覆盖 Source | Override Source | `override_sources` | Physical inputs applied after base sources to override logical units by primary index or field path. |
| Source 合成流程 | Source Composition Pipeline | `SourceComposition` | Deterministic process: combine `sources` first, then apply `override_sources`. |
| 拆分合并 | Split Merge | `SplitMerge` | Multiple base sources form one logical base table without overriding rows/fields. Unique-index conflicts are errors. |
| 覆盖应用 | Override Apply | `OverrideApply` | Override sources replace earlier logical units by primary index or field path. |
| Source 优先级 | Source Priority | `SourcePriority`, `priority` | Determines which override source wins when multiple override layers replace the same logical unit. |
| Source 冲突 | Source Conflict | `SourceConflict` | Invalid duplicate or ambiguous logical unit across sources. |
| 行级覆盖 | Row-Level Override | `RowOverride` | CollectionTable override granularity: one record selected by primary index key. |
| 字段级覆盖 | Field-Level Override | `FieldOverride` | ObjectTable override granularity: one field or field path. |
| 归一化 | Normalization | `Normalization`, `normalize` | Convert provider-specific sheet input into ConfigForge's internal logical model. |

## Preferred Naming for Table Forms

| 中文概念 | Preferred English | Preferred Rust Symbol | Reason |
| --- | --- | --- | --- |
| 对象表 / 列表 | Object Table | `ObjectTable`, `TableKind::Object` | The logical output is one object. `ListTable` is not recommended because `list` usually means an array-like container. |
| 集合表 / 行表 | Collection Table | `CollectionTable`, `TableKind::Collection` | The logical output is a collection of same-shaped records. `RowTable` is not recommended because every sheet has rows, so the name overemphasizes physical layout. |
| 表类型 | Table Kind | `TableKind` | `Kind` describes a domain discriminator and avoids confusion with field value types. |

Chinese aliases such as “对象表/列表” and “集合表/行表” are acceptable in product discussions. In code and durable design documents, prefer `Object Table` and `Collection Table` as the primary English terms.

## Preferred Naming for Field Model

| Concept | Preferred Code Symbol | Notes |
| --- | --- | --- |
| Field model | `FieldMeta` | One logical field definition. |
| Field type | `FieldType` | Logical value type, independent from sheet layout. |
| Primitive type | `PrimitiveFieldType` | `int`, `float`, `bool`, `string`, `text`, and `asset`. |
| Builtin value type | `BuiltinValueType` | Common one-cell composite values such as `color_rgba`, `vector3`, `rotation`, and `datetime`. |
| Color RGB | `ColorRgb` | Logical RGB color. |
| Color RGBA | `ColorRgba` | Logical RGBA color. |
| Vector2 / Vector3 / Vector4 | `Vector2`, `Vector3`, `Vector4` | Float vector builtin values. |
| Rotation | `Rotation` | Euler rotation builtin value. |
| DateTime | `DateTimeValue` | Parsed datetime builtin value. |
| DateTime pattern | `DateTimePattern` | Field-level datetime parsing pattern. |
| Duration | `DurationValue` | Parsed time span builtin value. |
| Range | `IntRange`, `FloatRange` | Inclusive numeric ranges. |
| Struct field | `FieldType::Struct`, `StructMeta` | Composite object-like field containing child fields. |
| Array field | `FieldType::Array`, `ArrayMeta` | Repeated field with an element type. |
| Field path | `FieldPath` | Stable path for nested mapping, validation diagnostics, and future code generation. |
| Sheet mapping | `SheetMapping` | Table-level physical mapping strategy. |
| Header mapping | `HeaderMapping` | Mapping from sheet headers to `FieldPath` values. |
| Field layout | `FieldLayout` | Field-level physical layout strategy. |
| Layout kind | `LayoutKind` | Suggested variants: `SingleCell`, `JsonCell`, `ExpandedColumns`, `ExpandedRows`. |
| Asset reference | `AssetRef` | Engine resource reference checked during export validation. |
| Localized text | `LocalizedText` | I18n-aware text value or future localization key/value representation. |

## Preferred Naming for Collection Access

| Concept | Preferred Code Symbol | Notes |
| --- | --- | --- |
| Collection access strategy | `CollectionAccessStrategy` | Runtime access shape for a CollectionTable. |
| Indexed lookup access | `IndexedAccess` | Access through declared indexes. |
| Source-ordered collection | `SourceOrderedCollection` | Access through preserved final source order. |
| Preserve source order flag | `preserve_source_order` | Recommended Meta field name for enabling ordered array access. |
| Ordered array export | `OrderedArrayExport` | Export records as an ordered runtime array. |
| Source order key | `SourceOrderKey` | Internal stable ordering key built from source order and row order. |

## Preferred Naming for Meta Syntax and Constraints

| Concept | Preferred Code Symbol | Notes |
| --- | --- | --- |
| In-memory Meta model | `MetaModel` or `MetaSet` | Canonical parsed object graph used by validators/exporters. |
| Meta document | `MetaDocument` | Physical authored file representation. |
| Meta syntax | `MetaSyntax` | Suggested variants: `Toml`, `Xml`. |
| XML schema | `XmlSchema` or `MetaSchema` | XSD used for IDE validation of XML Meta files. |
| Index definition | `IndexMeta` | Table-level lookup definition. |
| Index uniqueness | `IndexCardinality` | Suggested variants: `Unique`, `Multi`. |
| Index ordering | `IndexOrder` | Suggested variants: `Unordered`, `Ordered`. |
| Index storage | `IndexStorage` | Suggested variants: `HashMap`, `SortedArray`, `DenseArray`. |
| Primary index | `PrimaryIndex` | Default unique index for lookup and foreign-key target resolution. |
| Foreign key | `ForeignKey` | Field-level reference to another table's primary or named unique index; target may be single-field or composite. |
| Multi-source config | `sources`, `override_sources` | `sources` form the base split set; `override_sources` are applied as layers on top. |
| Source provider | `SourceProvider` | Suggested variants: `LocalFile`, `Csv`, `Xlsx`, `LarkSheet`, `GoogleSheet`; exact provider list should be extensible. |
| Source URI | `SourceUri` | Provider-specific locator, not interpreted by Meta validation beyond provider selection. |
| Source auth reference | `SourceAuthRef` | Credential reference only; secrets should not be embedded in Meta files. |
| Source composition | `SourceComposition` | Always runs split merge first, then override apply when override sources exist. |
| Source priority | `SourcePriority` | Explicit ordering for override resolution. |
| Polymorphic record | `PolymorphicRecord` | Runtime record composed from common fields plus zero or more fragments. |
| Fragment definition | `FragmentMeta` | Meta definition for one composable record fragment. |
| Fragment source | `FragmentSource` | Physical source table contributing records with specific fragments. |
| Enum definition | `EnumDef` | Meta-level definition of allowed enum variants. |
| Enum field type | `FieldType::Enum` or `EnumRef` | Field references a named enum. |
| Named struct definition | `NamedStruct` or `StructDef` | Reusable Meta-level struct definition. |
| Struct field reference | `FieldType::StructRef` or `StructRef` | Field references a named struct. |
| Struct dependency graph | `StructDependencyGraph` | Graph used to reject cyclic struct nesting. |
| Export configuration | `ExportConfig` | Export/codegen-owned settings, separate from Meta. |
| Export target | `ExportTarget` | One generated target such as TypeScript, Lua, JSON, or binary data. |
| Type mapping | `TypeMapping` | Maps logical Meta types to target-specific type names and runtime representations. |

## Preferred Naming for Enums, Named Structs, and Type Mapping

| Concept | Preferred Code Symbol | Notes |
| --- | --- | --- |
| Enum definition | `EnumDef` | Named finite value set in Meta. |
| Enum variant | `EnumVariant` | One allowed enum value; may include numeric/code value and display metadata. |
| Enum reference | `EnumRef` | Field-level reference to a named enum. |
| Named struct definition | `NamedStruct` or `StructDef` | Reusable struct declared once in Meta. |
| Inline struct | `InlineStruct` | Field-local struct shape for one-off data. |
| Struct reference | `StructRef` | Field-level reference to a named struct. |
| Struct dependency graph | `StructDependencyGraph` | Validation helper for named-struct reference cycles. |
| Struct cycle detection | `CycleDetection` | Validation step that rejects recursive value nesting. |
| Export configuration | `ExportConfig` | Root configuration for code/data exporters. |
| Export target | `ExportTarget` | One configured output target. |
| Type mapping | `TypeMapping` | Target-specific logical-to-runtime type mapping. |
| Type mapper | `TypeMapper` | Component that resolves mappings during code generation. |
| Target type | `TargetTypeName` | Exact type spelling in generated target code. |

## Preferred Naming for Validator System

| Concept | Preferred Code Symbol | Notes |
| --- | --- | --- |
| Validator | `Validator` | Executable correctness check. |
| Built-in validator | `BuiltInValidator` | Core checks implied by Meta semantics. |
| Configured validator | `ConfiguredValidator` | Project/business validator declared by configuration. |
| Field validator | `FieldValidator` | Checks one field value. |
| Record validator | `RecordValidator` | Checks one ObjectTable object or CollectionTable record. |
| Collection validator | `CollectionValidator` | Checks one final logical table. |
| Multi-collection validator | `MultiCollectionValidator` | Checks relationships across multiple tables. |
| Export validator | `ExportValidator` | Checks target-specific export correctness. |
| Validation scope | `ValidationScope` | Suggested variants: `Field`, `Record`, `Collection`, `MultiCollection`, `Project`, `Export`. |
| Validation phase | `ValidationPhase` | Suggested variants: `Meta`, `SourceDeclaration`, `SourceNormalization`, `Field`, `Record`, `SourceComposition`, `Collection`, `MultiCollection`, `Export`. |
| Validation context | `ValidationContext` | Shared read-only validation inputs. |
| Validation diagnostic | `ValidationDiagnostic` | Structured output for validation errors/warnings/info. |
| Validation severity | `ValidationSeverity` | Suggested variants: `Error`, `Warning`, `Info`. |
| Validator registry | `ValidatorRegistry` | Registry for validator kinds and implementations. |
| Validation plan | `ValidationPlan` | Deterministic execution plan. |
| Diagnostic code | `DiagnosticCode` | Stable machine-readable code. |

## Preferred Naming for Data and Runtime Export

| Concept | Preferred Code Symbol | Notes |
| --- | --- | --- |
| Source sheet | `SourceSheet` | Normalized two-dimensional sheet from any provider. |
| Sheet row | `SheetRow` | Row in a normalized source sheet. |
| Sheet cell | `SheetCell` | Raw cell with row/column and source location. |
| Data value | `DataValue` | Typed value parsed from sheet data according to Meta. |
| Data set | `DataSet` | Full typed logical data before target/context filtering. |
| Export dataset | `ExportDataset` | Target-specific filtered view passed to exporters. |
| Data exporter | `DataExporter` | Writes runtime data artifacts. |
| Runtime data format | `RuntimeDataFormat` | Suggested variants: `Json`, `Yaml`, `MessagePack`, `Protobuf`, `Sqlite`, `CustomBinary`. |
| Export artifact | `ExportArtifact` | One generated output file. |
| Artifact manifest | `ArtifactManifest` | Manifest of generated artifacts. |
| Special exporter | `SpecialExporter` | Creates auxiliary artifacts such as POT or asset manifest. |
| Gettext POT exporter | `GettextPotExporter` | Extracts `text` fields. |
| Asset manifest exporter | `AssetManifestExporter` | Extracts `asset` references. |
| Export context | `ExportContext` | Target, tags, and variables for one export run. |
| Export tag | `ExportTag` | Tag such as `shipping`, `dev`, `cn`, or `global`. |
| Export filter | `ExportFilter` | Context-dependent include/exclude rule. |
| Tool-only field | `tool_only`, `ToolOnlyField` | Parsed/validated but omitted from regular runtime data. |
| Runtime field | `RuntimeField` | Emitted to runtime data. |

## Preferred Naming for Implementation Infrastructure

| Concept | Preferred Code Symbol | Notes |
| --- | --- | --- |
| Plugin | `Plugin` | Generic extension capability, either built-in or dynamically loaded. |
| Dynamic plugin | `DynamicPlugin`, `PluginLibrary` | Dynamic library plugin loaded from `.dylib`, `.so`, or `.dll`. |
| Plugin manifest | `PluginManifest` | Stable plugin metadata and capability declaration. |
| Plugin capability | `PluginCapability` | Suggested variants: `SourceProvider`, `DataExporter`, `CodeGenerator`, `Validator`, `BuiltinType`, `SpecialExporter`. |
| Plugin registry | `PluginRegistry` | Coordinates plugin loading and registration. |
| Source provider registry | `SourceProviderRegistry` | Maps provider IDs to provider factories. |
| Data exporter registry | `DataExporterRegistry` | Maps runtime data formats to exporter factories. |
| Code generator registry | `CodeGeneratorRegistry` | Maps languages/targets to code generator factories. |
| Builtin type registry | `BuiltinTypeRegistry` | Maps builtin value type names to parser/validator providers. |
| Async source provider | `AsyncSourceProvider` | Long-term shape for source providers that may perform network I/O. |
| Async artifact writer | `AsyncArtifactWriter` or `ArtifactWriter` | Writes output artifacts and commits manifests asynchronously. |
| Task executor | `TaskExecutor` | Runs independent tasks in validation and export phases. |
| Execution plan | `ExecutionPlan`, `ValidationExecutionPlan`, `ExportExecutionPlan` | Deterministic plan used before parallel execution. |
| Diagnostic collector | `DiagnosticCollector` | Collects diagnostics during parsing, validation, source loading, and export. |
| Diagnostic reporter | `DiagnosticReporter` | Emits human-readable and machine-readable diagnostic output. |
| Diagnostic ordering | `DiagnosticOrdering` | Stable sort policy for diagnostics from parallel tasks. |
