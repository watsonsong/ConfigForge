# Target exports 与 Build DAG 重构 TODO

## 背景

当前 target 配置把导出任务拆成 `[[targets.data]]` 与 `[[targets.code]]`。Lua table 导出加入后，这个二分不再准确：Lua 产物是 `.lua` 代码文件，但内容内嵌数据，依赖 Meta + Data；gettext POT 依赖 Meta + Data，却不是 runtime data；Rust/C++/TypeScript codegen 只依赖 Meta。长期应把 target 下所有产物统一建模为 exporter task，并由 exporter dependency contract 驱动 Build DAG。

## 目标状态

1. Project Config canonical 语法使用 `[[targets.exports]]`。
2. 每个 export task 使用统一字段：`name`、`type`、`output` 与 exporter 专属参数；source data root 统一来自 `project.input`。
3. Exporter registry 声明 dependency contract：`MetaOnly` / `MetaAndData` / `DataOnly`。
4. Build 内部只处理 `NormalizedExportTask`，不再在编排层传播 data/code 二分。
5. Build 构造 target-local DAG，复用同 `(project.input, effective_tags)` 的 data load / normalize / validation。
6. Artifact 写出保持全局统一冲突检查，避免任一 exporter 产生部分落盘。

## 分阶段 TODO

### Step 1：配置与内部归一化模型先统一（本轮执行）

- [x] 在 `src/build/target.rs` 引入 `NormalizedExportTask` 与 `ExportDependency`。
- [x] Project Config target task canonical 语法改为 `[[targets.exports]]`。
- [x] 移除 `[[targets.data]]` / `[[targets.code]]` 兼容设计与测试期望。
- [x] normalize 阶段直接从 `TargetConfig.exports` 生成统一 `exports` 列表。
- [x] Build 主流程从 `target.exports` 派生当前 runtime-data/codegen 执行视图，先保持执行阶段行为不变。
- [x] 测试覆盖：单 export task 默认名、重复 export task 名、export task 禁止 `input`、MetaAndData task 依赖 `project.input`。

### Step 2：Exporter registry 增加 dependency contract

- [x] 为 runtime table exporter、aggregate data exporter、code generator 增加 dependency contract。
- [x] 每个 exporter/generator 通过 name/language、aliases 与 dependency 参与 registry 查询；config parser 仍由现有 build adapter 负责。
- [x] `json` / `messagepack` / `bin` / `gettext_pot` / `lua_table` 标记为 `MetaAndData`。
- [x] `rust` / `cpp` / `typescript` 标记为 `MetaOnly`。

### Step 3：Build DAG executor MVP

- [ ] 抽象 DAG node key 和 node result。
- [ ] 实现 target-local DAG 拓扑调度。
- [ ] 保证 `MetaOnly` exporter 与 data load 可并行。
- [ ] 保证同 `(project.input, effective_tags)` 的 `MetaAndData` exporter 共享 DataSet / validation。
- [ ] 保证 artifact 顺序按 export task 声明顺序稳定。

### Step 4：文档、fixture 与清理

- [ ] 文档示例统一改为 `targets.exports`。
- [ ] standard_project 与所有 integration fixture 使用新语法。
- [ ] CLI integration 覆盖 export task 同名冲突、路径冲突、DAG 复用。
- [ ] 删除旧 data/code task 命名和兼容路径相关代码。

## 风险与约束

- `DataLoadPlan` / `DataSet` cache key 必须包含 effective tags，不能只按 `project.input` 缓存。
- Codegen 当前格式无关，不能重新引入 code task 绑定具体 data task 的语义。
- Artifact path 冲突必须在所有 exporter 产物汇总后统一检查。
- 项目尚未公开，不需要兼容旧配置；实现应优先保持内部模型干净。
