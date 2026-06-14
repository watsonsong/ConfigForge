// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::DiagnosticSeverity;

pub const CF_GENERATOR_UNKNOWN_TYPE: &str = "CF_GENERATOR_UNKNOWN_TYPE";
pub const CF_GENERATOR_AMBIGUOUS_TYPE: &str = "CF_GENERATOR_AMBIGUOUS_TYPE";
pub const CF_GENERATOR_INVALID_OPTION: &str = "CF_GENERATOR_INVALID_OPTION";
pub const CF_GENERATOR_UNSUPPORTED_OPTION: &str = "CF_GENERATOR_UNSUPPORTED_OPTION";
pub const CF_GENERATOR_UNSUPPORTED_DATA_INPUT: &str = "CF_GENERATOR_UNSUPPORTED_DATA_INPUT";
pub const CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH: &str = "CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH";

pub const CF_ARTIFACT_PATH_ESCAPE: &str = "CF_ARTIFACT_PATH_ESCAPE";
pub const CF_ARTIFACT_PATH_COLLISION: &str = "CF_ARTIFACT_PATH_COLLISION";
pub const CF_ARTIFACT_WRITE_STATE: &str = "CF_ARTIFACT_WRITE_STATE";
pub const CF_ARTIFACT_CASE_INSENSITIVE_COLLISION: &str = "CF_ARTIFACT_CASE_INSENSITIVE_COLLISION";
pub const CF_ARTIFACT_SYMLINK_ALIAS_COLLISION: &str = "CF_ARTIFACT_SYMLINK_ALIAS_COLLISION";
pub const CF_ARTIFACT_HARDLINK_ALIAS_COLLISION: &str = "CF_ARTIFACT_HARDLINK_ALIAS_COLLISION";

pub const CF_DATA_INVALID_TYPE: &str = "CF_DATA_INVALID_TYPE";
pub const CF_DATA_MISSING_REQUIRED_FIELD: &str = "CF_DATA_MISSING_REQUIRED_FIELD";
pub const CF_DATA_INVALID_ENUM: &str = "CF_DATA_INVALID_ENUM";
pub const CF_DATA_INVALID_FOREIGN_KEY: &str = "CF_DATA_INVALID_FOREIGN_KEY";
pub const CF_DATA_INVALID_ASSET_REF: &str = "CF_DATA_INVALID_ASSET_REF";
pub const CF_DATA_DUPLICATE_KEY: &str = "CF_DATA_DUPLICATE_KEY";
pub const CF_DATA_SOURCE_READ_FAILED: &str = "CF_DATA_SOURCE_READ_FAILED";
pub const CF_DATA_SOURCE_SCHEMA_INVALID: &str = "CF_DATA_SOURCE_SCHEMA_INVALID";
pub const CF_DATA_SOURCE_PATH_ESCAPE: &str = "CF_DATA_SOURCE_PATH_ESCAPE";
pub const CF_DATA_DUPLICATE_HEADER: &str = "CF_DATA_DUPLICATE_HEADER";

pub const CF_VALIDATION_INVALID_CONFIG: &str = "CF_VALIDATION_INVALID_CONFIG";

pub const CF_META_DUPLICATE_TABLE_NAME: &str = "CF_META_DUPLICATE_TABLE_NAME";
pub const CF_META_DUPLICATE_ENUM_NAME: &str = "CF_META_DUPLICATE_ENUM_NAME";
pub const CF_META_DUPLICATE_STRUCT_NAME: &str = "CF_META_DUPLICATE_STRUCT_NAME";
pub const CF_META_DUPLICATE_ENUM_VALUE_NAME: &str = "CF_META_DUPLICATE_ENUM_VALUE_NAME";
pub const CF_META_DUPLICATE_FIELD_NAME: &str = "CF_META_DUPLICATE_FIELD_NAME";
pub const CF_META_DUPLICATE_INDEX_NAME: &str = "CF_META_DUPLICATE_INDEX_NAME";
pub const CF_META_DUPLICATE_TYPE_NAME: &str = "CF_META_DUPLICATE_TYPE_NAME";
pub const CF_META_RESERVED_TYPE_NAME: &str = "CF_META_RESERVED_TYPE_NAME";
pub const CF_META_MISSING_FIELD_REFERENCE: &str = "CF_META_MISSING_FIELD_REFERENCE";
pub const CF_META_INVALID_PRIMARY_INDEX: &str = "CF_META_INVALID_PRIMARY_INDEX";
pub const CF_META_PRIMARY_INDEX_MISMATCH: &str = "CF_META_PRIMARY_INDEX_MISMATCH";
pub const CF_META_EMPTY_INDEX_FIELDS: &str = "CF_META_EMPTY_INDEX_FIELDS";
pub const CF_META_UNKNOWN_FIELD_TYPE: &str = "CF_META_UNKNOWN_FIELD_TYPE";
pub const CF_META_INVALID_DEFAULT_VALUE: &str = "CF_META_INVALID_DEFAULT_VALUE";
pub const CF_META_INVALID_SOURCE_DECLARATION: &str = "CF_META_INVALID_SOURCE_DECLARATION";
pub const CF_META_INVALID_VALIDATOR_CONFIG: &str = "CF_META_INVALID_VALIDATOR_CONFIG";
pub const CF_META_INVALID_FOREIGN_KEY_DECLARATION: &str = "CF_META_INVALID_FOREIGN_KEY_DECLARATION";
pub const CF_META_DUPLICATE_ASSET_KIND_NAME: &str = "CF_META_DUPLICATE_ASSET_KIND_NAME";
pub const CF_META_INVALID_ASSET_KIND_DECLARATION: &str = "CF_META_INVALID_ASSET_KIND_DECLARATION";
pub const CF_META_UNSUPPORTED_FEATURE: &str = "CF_META_UNSUPPORTED_FEATURE";

pub const CF_BUILD_TARGET_NOT_FOUND: &str = "CF_BUILD_TARGET_NOT_FOUND";
pub const CF_BUILD_INVALID_CONFIG: &str = "CF_BUILD_INVALID_CONFIG";
pub const CF_BUILD_INVALID_OPTION: &str = "CF_BUILD_INVALID_OPTION";
pub const CF_BUILD_INPUT_REQUIRED: &str = "CF_BUILD_INPUT_REQUIRED";
pub const CF_BUILD_EXPORT_TASK_FAILED: &str = "CF_BUILD_EXPORT_TASK_FAILED";

pub const CF_PROJECT_INVALID_CONFIG: &str = "CF_PROJECT_INVALID_CONFIG";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticCodeMetadata {
    pub code: &'static str,
    pub default_severity: DiagnosticSeverity,
    pub domain: &'static str,
    pub title: &'static str,
    pub explanation: &'static str,
    pub default_hint: Option<&'static str>,
    pub since: &'static str,
    pub deprecated: bool,
}

impl DiagnosticCodeMetadata {
    const fn error(
        code: &'static str,
        domain: &'static str,
        title: &'static str,
        explanation: &'static str,
        default_hint: Option<&'static str>,
        since: &'static str,
    ) -> Self {
        Self {
            code,
            default_severity: DiagnosticSeverity::Error,
            domain,
            title,
            explanation,
            default_hint,
            since,
            deprecated: false,
        }
    }

    const fn legacy_error(
        code: &'static str,
        domain: &'static str,
        title: &'static str,
        explanation: &'static str,
        default_hint: Option<&'static str>,
        since: &'static str,
    ) -> Self {
        Self {
            deprecated: true,
            ..Self::error(code, domain, title, explanation, default_hint, since)
        }
    }
}

pub const DIAGNOSTIC_CODE_METADATA: &[DiagnosticCodeMetadata] = &[
    DiagnosticCodeMetadata::error(
        CF_GENERATOR_UNKNOWN_TYPE,
        "generator",
        "未知导出生成器类型",
        "当前 export task 的 type_id 没有注册对应 generator。",
        Some("检查 targets.exports.type，或确认对应 generator 是否已接入 registry。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_GENERATOR_AMBIGUOUS_TYPE,
        "generator",
        "导出生成器类型存在歧义",
        "多个 generator 注册了相同 type_id 或 alias，导致无法稳定解析。",
        Some("检查 generator descriptor，确保 type_id 与 alias 全局唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_GENERATOR_INVALID_OPTION,
        "generator",
        "导出配置选项格式错误",
        "generator option 的类型或格式无法按该 generator 的配置 schema 解析。",
        Some("检查该 export task 的 options 类型和值。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_GENERATOR_UNSUPPORTED_OPTION,
        "generator",
        "导出配置选项不受支持",
        "export task 声明了当前 generator 不识别的 option。",
        Some("删除未知 option，或确认该 generator 是否支持该配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
        "generator",
        "导出生成器输入不满足要求",
        "generator 运行时需要的 Meta/Data/ValidationReport 输入与当前 build 分支不匹配。",
        Some("检查 generator requirements 与 target export 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_GENERATOR_RUNTIME_CONTRACT_MISMATCH,
        "generator",
        "运行时读取契约不匹配",
        "生成代码或 runtime helper 要求的 reader contract 与当前导出产物不一致。",
        Some("确认生成代码、runtime reader 与导出数据使用同一 contract 版本。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_ARTIFACT_PATH_ESCAPE,
        "artifact",
        "产物路径逃逸输出根目录",
        "artifact 输出路径解析后不在配置的 output root 内。",
        Some("使用相对输出路径，并避免绝对路径或 parent directory 逃逸。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_ARTIFACT_PATH_COLLISION,
        "artifact",
        "产物路径冲突",
        "多个 artifact 解析到相同的 normalized 输出路径。",
        Some("调整 export output/file_name，确保每个 artifact 目标路径唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_ARTIFACT_WRITE_STATE,
        "artifact",
        "产物写入状态无效",
        "artifact sink 或 stream 的生命周期状态不允许继续写入或完成。",
        Some(
            "检查 artifact stream 是否已 finish，或 sink 是否已因先前错误进入 failed/closed 状态。",
        ),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_ARTIFACT_CASE_INSENSITIVE_COLLISION,
        "artifact",
        "产物路径大小写冲突",
        "多个 artifact 在大小写不敏感文件系统上会解析到同一路径。",
        Some("调整文件名大小写或路径，避免跨平台冲突。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_ARTIFACT_SYMLINK_ALIAS_COLLISION,
        "artifact",
        "产物路径符号链接别名冲突",
        "多个 artifact 通过 symlink alias 指向同一文件系统位置。",
        Some("避免 output root 内的 symlink alias，或调整输出路径。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_ARTIFACT_HARDLINK_ALIAS_COLLISION,
        "artifact",
        "产物路径硬链接别名冲突",
        "多个 artifact 通过 hardlink alias 指向同一文件系统位置。",
        Some("避免 output root 内的 hardlink alias，或调整输出路径。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_INVALID_TYPE,
        "data",
        "字段类型解析失败",
        "源数据无法按 Meta 字段类型解析。",
        Some("检查单元格内容是否符合字段类型、枚举值或复合值格式。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_MISSING_REQUIRED_FIELD,
        "data",
        "必填字段缺失",
        "字段声明 required 且没有 default，但当前数据未提供值。",
        Some("补齐该字段，或在 Meta 中配置 default / 调整 required。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_INVALID_ENUM,
        "data",
        "枚举值非法",
        "字段值不在 Meta 声明的 enum values 中。",
        Some("改为合法枚举值，或确认 Meta enum 是否需要新增值。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_INVALID_FOREIGN_KEY,
        "data",
        "外键引用无效",
        "当前字段引用的目标表记录不存在或不满足 target-aware 外键规则。",
        Some("检查引用值、目标表主键/唯一索引，以及当前 target 下目标记录是否可见。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_INVALID_ASSET_REF,
        "data",
        "资源引用无效",
        "asset 字段无法通过 asset_kind 和 search_paths 解析到合法资源。",
        Some("检查资源路径、文件是否存在、asset_kind 规则和 target tags。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_DUPLICATE_KEY,
        "data",
        "唯一性约束冲突",
        "字段或索引要求唯一，但当前数据存在重复 key。",
        Some("修改重复行的 key 字段，或确认 unique/index 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_SOURCE_READ_FAILED,
        "data",
        "源数据读取失败",
        "CSV 或其他 source provider 读取源文件时发生 IO、解析或行列格式错误。",
        Some("检查源文件是否存在、CSV 行列数量是否一致，以及文件编码/格式是否正确。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_SOURCE_SCHEMA_INVALID,
        "data",
        "源数据配置或表头无效",
        "source layout、header row、provider 或 ObjectTable key/value 表头不满足当前读取规则。",
        Some("检查 Meta 中 source 配置、CSV 表头和 row layout。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_SOURCE_PATH_ESCAPE,
        "data",
        "源数据路径逃逸输入根目录",
        "source path 解析后不在 project.input / data root 内，或通过文件系统别名逃逸。",
        Some("使用 data root 内的相对 source path，避免绝对路径、.. 或 symlink alias。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_DATA_DUPLICATE_HEADER,
        "data",
        "源数据表头重复",
        "CSV header row 中存在重复列名，无法稳定映射字段值。",
        Some("修改重复列名，确保同一 source header row 内每个列名唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_BUILD_TARGET_NOT_FOUND,
        "build",
        "构建目标不存在",
        "请求的 target 没有在 Project Config 中声明。",
        Some("检查 CLI --target 或 targets.name 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_BUILD_INVALID_CONFIG,
        "build",
        "构建配置无效",
        "target export task、validation report 附加任务或 build pipeline 配置不满足构建约束。",
        Some("检查 target exports、task name、input/data 选项和 validation report 输出路径。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_BUILD_INVALID_OPTION,
        "build",
        "构建执行选项无效",
        "CLI 或构建调用传入的执行资源选项超出允许范围。",
        Some("检查 --jobs、--io-concurrency、--network-concurrency 或 --serial 参数。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_BUILD_INPUT_REQUIRED,
        "build",
        "构建输入缺失",
        "当前 build pipeline 需要的输入尚未准备完成。",
        Some("检查 target、source 和 export task 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_BUILD_EXPORT_TASK_FAILED,
        "build",
        "导出任务失败",
        "某个 export task 执行失败。",
        Some("查看 source/target 与 task name，定位失败的导出任务。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_PROJECT_INVALID_CONFIG,
        "project",
        "项目配置无效",
        "Project Config 中存在重复声明、未知引用或当前不支持的配置值。",
        Some("检查 ConfigForge.toml 中 targets、assets.search_paths 和 asset_kinds 等声明。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_VALIDATION_INVALID_CONFIG,
        "validation",
        "校验器配置无效",
        "内置 validator 的作用域、参数类型、字段引用或规则组合无效。",
        Some("检查 validator kind、声明位置、参数名和参数值类型。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_TABLE_NAME,
        "meta",
        "Meta 表名重复",
        "Meta 中声明了重复的 table name，导致后续 link 和导出无法稳定解析表引用。",
        Some("检查 [[tables]] 的 name，确保同一 MetaModel 内全局唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_ENUM_NAME,
        "meta",
        "Meta 枚举名重复",
        "Meta 中声明了重复的 enum name，导致字段类型解析存在歧义。",
        Some("检查 [[enums]] 的 name，确保类型名全局唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_STRUCT_NAME,
        "meta",
        "Meta 结构体名重复",
        "Meta 中声明了重复的 struct name，导致字段类型解析存在歧义。",
        Some("检查 [[structs]] 的 name，确保类型名全局唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_ENUM_VALUE_NAME,
        "meta",
        "Meta 枚举值名重复",
        "同一 enum 中存在重复的 value name，导致数据枚举值无法唯一解析。",
        Some("检查 enum values，确保每个 value name 在所属 enum 内唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_FIELD_NAME,
        "meta",
        "Meta 字段名重复",
        "同一 table 或 struct 中存在重复字段名，导致数据列映射或类型展开存在歧义。",
        Some("检查 fields.name，确保字段名在所属对象内唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_INDEX_NAME,
        "meta",
        "Meta 索引名重复",
        "同一 table 中存在重复 index name，导致生成查询接口和约束校验存在歧义。",
        Some("检查 indexes.name，确保索引名在所属 table 内唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_TYPE_NAME,
        "meta",
        "Meta 类型名重复",
        "enum、struct、table 或内置类型之间存在重复或冲突的类型名。",
        Some("检查所有自定义类型名称，并避免与内置 primitive 类型重名。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_RESERVED_TYPE_NAME,
        "meta",
        "Meta 使用保留类型名",
        "Meta 自定义类型使用了 ConfigForge 保留或内置的类型名。",
        Some("重命名自定义 enum、struct 或 table，避免使用 primitive / reserved 类型名。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_MISSING_FIELD_REFERENCE,
        "meta",
        "Meta 字段引用缺失",
        "index、primary key、foreign key、validator 或 source layout 引用了不存在的字段。",
        Some("检查相关 fields 列表或 field path，确保引用的字段已在 table/struct 中声明。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_INVALID_PRIMARY_INDEX,
        "meta",
        "Meta 主索引无效",
        "CollectionTable 的 primary index 声明不满足主键约束或引用了非法字段。",
        Some("检查 primary_index 与 indexes 声明，确保主索引字段存在且可作为唯一主键。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_PRIMARY_INDEX_MISMATCH,
        "meta",
        "Meta 主索引不一致",
        "primary index 与表字段或索引声明不一致，无法建立稳定的主键语义。",
        Some("检查 primary_index 名称、字段列表以及 indexes 中对应索引声明。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_EMPTY_INDEX_FIELDS,
        "meta",
        "Meta 索引字段为空",
        "index 未声明任何字段，无法参与唯一性检查或生成查询接口。",
        Some("为 index.fields 至少配置一个字段，或删除无效 index。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_UNKNOWN_FIELD_TYPE,
        "meta",
        "Meta 字段类型未知",
        "字段声明的 field_type 既不是内置 primitive，也不是已声明的 enum 或 struct。",
        Some("检查字段 type 拼写，或先声明对应 enum / struct。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_INVALID_DEFAULT_VALUE,
        "meta",
        "Meta 默认值无效",
        "字段 default 无法按字段类型解析，或 default 与字段约束不兼容。",
        Some("检查 default 字符串是否符合字段类型、枚举值或复合值格式。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_INVALID_SOURCE_DECLARATION,
        "meta",
        "Meta source 声明无效",
        "table 或 field 的 source layout、provider、路径或列映射配置不满足读取约束。",
        Some("检查 source 配置、ObjectTable layout、header row 和 field path。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_INVALID_VALIDATOR_CONFIG,
        "meta",
        "Meta validator 配置无效",
        "validator 的 kind、作用域、字段引用或参数组合不满足内置校验器要求。",
        Some("检查 validator kind、params、字段引用和 table/field 作用域。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_INVALID_FOREIGN_KEY_DECLARATION,
        "meta",
        "Meta foreign key 声明无效",
        "foreign key 的 from/to 字段、目标表、目标索引或 target-aware 规则不满足结构约束。",
        Some("检查 foreign_keys 配置、引用表和字段是否存在，并确认字段数量与类型匹配。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_DUPLICATE_ASSET_KIND_NAME,
        "meta",
        "Meta asset kind 名重复",
        "Meta 中声明了重复的 asset kind name，导致 asset 字段规则无法唯一解析。",
        Some("检查 asset_kinds.name，确保每个 asset kind 全局唯一。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_INVALID_ASSET_KIND_DECLARATION,
        "meta",
        "Meta asset kind 声明无效",
        "asset kind 的继承、路径规则、扩展名策略或正则规则不满足结构约束。",
        Some("检查 asset_kinds 的 extends、search_paths、extensions、regex_rules 等配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::error(
        CF_META_UNSUPPORTED_FEATURE,
        "meta",
        "Meta 使用未支持特性",
        "Meta 声明了当前版本尚未支持或当前组合下不可用的配置能力。",
        Some("查看对应配置项是否已实现，或调整为当前版本支持的声明方式。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "field.range_violation",
        "validation",
        "数值超出允许范围",
        "该字段值不满足 range validator 配置的上下界。",
        Some(
            "检查当前值、min/max/exclusive_min/exclusive_max 配置，确认数据是否填错或规则是否需要调整。",
        ),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.range_violation",
        "validation",
        "数值超出允许范围",
        "该字段值不满足 range validator 配置的上下界。",
        Some(
            "检查当前值、min/max/exclusive_min/exclusive_max 配置，确认数据是否填错或规则是否需要调整。",
        ),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.regex_mismatch",
        "validation",
        "文本格式不符合命名规则",
        "该字段值没有匹配 regex validator 配置的正则表达式。",
        Some("按规则调整字段文本，或确认 pattern/match_mode/case_sensitive 配置是否正确。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.string_too_short",
        "validation",
        "文本长度过短",
        "该字段文本长度小于 string_length validator 配置的最小长度。",
        Some("补充文本内容，或调整 min / exact 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.string_too_long",
        "validation",
        "文本长度过长",
        "该字段文本长度大于 string_length validator 配置的最大长度。",
        Some("缩短文本内容，或调整 max / exact 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.string_length_mismatch",
        "validation",
        "文本长度不等于期望值",
        "该字段文本长度不等于 string_length validator 配置的 exact 长度。",
        Some("调整文本长度，或确认 exact 配置是否符合需求。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.required_when_missing",
        "validation",
        "条件满足时缺少必填字段",
        "required_when validator 的 when 条件已满足，但目标字段为空。",
        Some("补充目标字段，或检查 when 条件与字段可见性配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.compare_failed",
        "validation",
        "字段间比较关系不成立",
        "compare validator 检查的两个字段不满足配置的比较操作。",
        Some("检查左右字段值，确认 op 方向和空值策略是否正确。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.compare_operand_missing",
        "validation",
        "字段比较缺少操作数",
        "compare validator 需要参与比较的字段值，但当前行缺少对应值。",
        Some("补齐参与比较的字段，或启用 skip_if_either_empty。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.sum_mismatch",
        "validation",
        "分组总和不等于期望值",
        "sum validator 计算出的字段总和与 equals 配置不一致。",
        Some("检查该组每行数值、group_by 字段和 epsilon 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.sum_too_small",
        "validation",
        "分组总和过小",
        "sum validator 计算出的字段总和小于 min 配置。",
        Some("增加该组数值，或检查 min / group_by 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.sum_too_large",
        "validation",
        "分组总和过大",
        "sum validator 计算出的字段总和大于 max 配置。",
        Some("降低该组数值，或检查 max / group_by 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.monotonic_violation",
        "validation",
        "序列单调性被破坏",
        "monotonic validator 按 order_by 排序后发现字段值不满足递增/递减规则。",
        Some("检查当前行和上一行的排序键与字段值，确认 direction/strict 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "validator.monotonic_duplicate_order_key",
        "validation",
        "排序键重复",
        "monotonic validator 遇到重复 order_by 值，且未允许 skip_equal_order_key。",
        Some("修正重复排序键，或确认是否应该启用 skip_equal_order_key。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "field.required",
        "data",
        "必填字段缺失",
        "字段声明 required 且没有 default，但当前数据未提供值。",
        Some("补齐该字段，或在 Meta 中配置 default / 调整 required。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "field.type",
        "data",
        "字段类型解析失败",
        "源数据无法按 Meta 字段类型解析。",
        Some("检查单元格内容是否符合字段类型、枚举值或复合值格式。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "field.enum",
        "data",
        "枚举值非法",
        "字段值不在 Meta 声明的 enum values 中。",
        Some("改为合法枚举值，或确认 Meta enum 是否需要新增值。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "field.unique",
        "data",
        "唯一性约束冲突",
        "字段或索引要求唯一，但当前数据存在重复 key。",
        Some("修改重复行的 key 字段，或确认 unique/index 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "index.unique",
        "data",
        "唯一性约束冲突",
        "字段或索引要求唯一，但当前数据存在重复 key。",
        Some("修改重复行的 key 字段，或确认 unique/index 配置。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "foreign_key.missing",
        "data",
        "外键引用无效",
        "当前字段引用的目标表记录不存在或不满足 target-aware 外键规则。",
        Some("检查引用值、目标表主键/唯一索引，以及当前 target 下目标记录是否可见。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "foreign_key.invalid",
        "data",
        "外键引用无效",
        "当前字段引用的目标表记录不存在或不满足 target-aware 外键规则。",
        Some("检查引用值、目标表主键/唯一索引，以及当前 target 下目标记录是否可见。"),
        "0.1.0",
    ),
    DiagnosticCodeMetadata::legacy_error(
        "asset.reference",
        "data",
        "资源引用无效",
        "asset 字段无法通过 asset_kind 和 search_paths 解析到合法资源。",
        Some("检查资源路径、文件是否存在、asset_kind 规则和 target tags。"),
        "0.1.0",
    ),
];

pub fn metadata_for(code: &str) -> Option<&'static DiagnosticCodeMetadata> {
    DIAGNOSTIC_CODE_METADATA
        .iter()
        .find(|metadata| metadata.code == code)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const VALID_DOMAINS: &[&str] = &[
        "artifact",
        "build",
        "data",
        "generator",
        "meta",
        "project",
        "validation",
    ];

    #[test]
    fn diagnostic_code_catalog_has_unique_codes_and_required_metadata() {
        let mut codes = BTreeSet::new();
        for metadata in DIAGNOSTIC_CODE_METADATA {
            assert!(
                codes.insert(metadata.code),
                "duplicate code `{}`",
                metadata.code
            );
            assert!(
                VALID_DOMAINS.contains(&metadata.domain),
                "invalid diagnostic domain `{}` for `{}`",
                metadata.domain,
                metadata.code
            );
            assert!(!metadata.title.trim().is_empty());
            assert!(!metadata.explanation.trim().is_empty());
            assert!(!metadata.since.trim().is_empty());
            assert_eq!(metadata.default_severity, DiagnosticSeverity::Error);
        }
    }

    #[test]
    fn catalog_contains_first_stable_codes_and_legacy_validation_codes() {
        assert_eq!(
            metadata_for(CF_GENERATOR_UNKNOWN_TYPE).map(|metadata| metadata.domain),
            Some("generator")
        );
        assert_eq!(
            metadata_for(CF_DATA_INVALID_TYPE).map(|metadata| metadata.title),
            Some("字段类型解析失败")
        );
        assert_eq!(
            metadata_for(CF_META_DUPLICATE_TABLE_NAME).map(|metadata| metadata.domain),
            Some("meta")
        );
        assert_eq!(
            metadata_for(CF_META_UNKNOWN_FIELD_TYPE).map(|metadata| metadata.title),
            Some("Meta 字段类型未知")
        );
        assert_eq!(
            metadata_for(CF_META_INVALID_VALIDATOR_CONFIG).map(|metadata| metadata.domain),
            Some("meta")
        );
        assert!(metadata_for("field.type").is_some_and(|metadata| metadata.deprecated));
        assert!(metadata_for("validator.sum_mismatch").is_some_and(|metadata| metadata.deprecated));
    }
}
