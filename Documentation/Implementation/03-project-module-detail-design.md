# 03. Project 模块详细设计

本文档描述 `src/project/` 的实现边界。Project 模块负责工程配置文件的读取、imports 合并、路径归一化、target 配置聚合和 Meta 抽取；它不负责任务执行，也不直接加载数据或生成产物。

## 3.1 设计目标

Project 模块解决以下问题：

1. 从 `ConfigForge.toml` 读取工程配置。
2. 支持 Project Config 顶层 `imports`，递归加载配置片段并通过 canonical path 去重。
3. 将不同配置片段中的 `project`、`enums`、`structs`、`tables`、`targets` 合并成一个 `ProjectConfig`。
4. 将相对路径按声明文件所在目录归一化，避免 imports 后路径相对根目录漂移。
5. 保留 target 聚合结构：每个 target 可声明多个 runtime data task 和多个 codegen task。
6. 从工程配置中抽取 `MetaModel`，并在返回前完成 meta link / validate 的基础校验。

非目标：

- 不执行 build；执行编排属于 `src/build/`。
- 不读取 CSV 数据；数据读取属于 `src/data/`。
- 不序列化 runtime data；数据格式导出属于 `src/exporter/`。
- 不生成代码；代码生成属于 `src/codegen/`。

## 3.2 模块结构

当前 Project 模块集中在：

```text
src/project/
  mod.rs              # ProjectConfig / TargetConfig / imports / path normalize / tests
```

后续如果配置能力继续增长，可拆分为：

```text
src/project/
  mod.rs              # 对外 API 与 re-export
  model.rs            # ProjectConfig / TargetConfig
  loader.rs           # from_path / recursive imports
  merge.rs            # config fragment merge
  path.rs             # lexical path normalize
  validate.rs         # target name / task-level project validation
  tests.rs
```

## 3.3 核心类型

```rust
pub struct ProjectConfig {
    pub imports: Vec<PathBuf>,
    pub project: ProjectConfigMeta,
    pub enums: Vec<EnumMeta>,
    pub structs: Vec<StructMeta>,
    pub tables: Vec<TableMeta>,
    pub targets: Vec<TargetConfig>,
}
```

`ProjectConfig` 是合并后的工程视图。Project Config 允许直接承载 Meta 片段，因此 `enums`、`structs`、`tables` 的数据结构来自 `meta` 模块。

```rust
pub struct TargetConfig {
    pub name: String,
    pub tags: Vec<String>,
    pub data: Vec<TargetDataConfig>,
    pub code: Vec<CodegenTaskConfig>,
}
```

`TargetConfig` 是 project-level 聚合结构，可以引用：

- `exporter::TargetDataConfig`：runtime data task 配置；
- `codegen::CodegenTaskConfig`：code task 配置。

这是一种配置层聚合依赖，不代表 Project 模块执行 exporter 或 codegen。

## 3.4 Imports 与合并规则

加载流程：

```text
ProjectConfig::from_path(root)
  -> canonicalize(root)
  -> read TOML
  -> recursively load imports relative to current file
  -> normalize paths relative to each declaring file
  -> append fragment
  -> merge fragments in load order
  -> validate duplicate target names
```

合并规则：

- `imports` 仅控制加载，不进入最终语义。
- `project.name` / `desc` / `copyright` 后加载片段覆盖先加载片段的非空值。
- `enums` / `structs` / `tables` / `targets` 按片段加载顺序 append。
- 重复 table / enum / struct 等 Meta 语义错误不在 Project merge 阶段处理，而交由 Meta validate 输出 `MetaDiagnostic`。
- 重复 target name 是 Project 层错误，因为 target 不属于 MetaModel。

## 3.5 路径归一化

Project 模块负责将 target task 中的路径归一化：

- data task `input`；
- data task `output`；
- code task `output`。

路径以声明它的配置片段所在目录为 base，而不是以 root project 文件为 base。这样被 imports 的配置片段可以自包含地声明相对路径。

`lexical_normalize_path()` 只做词法归一化，折叠 `.` 和可安全折叠的 `..`，不要求目标路径已经存在。

## 3.6 与 Meta / Build 的关系

Project 模块位于 Meta 与 Build 之间：

```text
build
  -> project
      -> meta       # 使用 Meta 数据结构和校验入口
      -> exporter   # 仅引用 TargetDataConfig
      -> codegen    # 仅引用 CodegenTaskConfig
```

关键约束：

- `project` 不能依赖 `build`。
- `project` 不调用 `DataLoader`，不创建线程池。
- `project.meta_model()` 返回 `MetaModel` 前可以调用 meta link/validate 做 fail-fast，但不持有 `LinkedMetaModel<'meta>`，因为 linked view 的生命周期属于调用方。

## 3.7 测试边界

Project 模块测试应覆盖：

- imports 递归加载和去重。
- 不同目录配置片段中的相对路径归一化。
- target name 重复时报错。
- data format 默认值属于 exporter config serde 行为，但可通过 project 解析测试覆盖。
- path lexical normalize 对 absolute root、leading parent、`.` / `..` 的处理。
