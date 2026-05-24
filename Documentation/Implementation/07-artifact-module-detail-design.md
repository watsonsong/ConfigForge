# 07. Artifact 模块详细设计

本文档描述 `src/artifact/` 模块的实现边界。Artifact 模块负责统一管理 ConfigForge 的所有文件制品：runtime data、generated code、未来 special exporter 输出等。Exporter 和 Codegen 只负责生成制品内容与相对目标路径；Build 负责收集制品并调用 Artifact Writer 执行路径冲突检查、dry-run、direct 写入或 temp-commit 写入。

## 7.1 设计目标

Artifact 模块需要解决：

1. 统一 runtime data 与 generated code 的制品模型，避免 exporter / codegen 各自定义写文件流程。
2. 在写入前集中检查输出路径冲突，覆盖跨 data task、跨 code task、runtime data 与 generated code 之间的冲突。
3. 支持 `dry-run`：完整执行配置加载、Meta link、data load、validation、serialization/codegen 和路径规划，但不写入最终文件。
4. 支持 `direct` 与 `temp-commit` 两种写入策略，并支持 `dry-run` 写入计划。
5. 生成稳定的写入结果：路径稳定排序、bytes/文本内容统一表达。
6. 保持模块依赖方向简单：artifact 是底层服务模块，不依赖 exporter、codegen、build、project、data、validation。

非目标：

- 不决定哪些 runtime data / codegen task 应该执行；task 编排属于 `build`。
- 不理解 Meta、DataSet、TargetTags 或运行时格式；语义生成属于 exporter / codegen。
- 不负责 CLI 参数解析；CLI 只把 dry-run / write mode 传给 build options。
- 第一阶段不实现远端上传、增量缓存、分布式 artifact store。

## 7.2 模块结构

建议新增目录模块：

```text
src/artifact/
  mod.rs          # 对外 API 与 re-export
  model.rs        # Artifact / ArtifactContent / ArtifactPath
  writer.rs       # ArtifactWriter / ArtifactWriteMode / dry-run / direct / temp-commit
  tests.rs        # 路径冲突、dry-run、写入策略单元测试
```

第一阶段可以先合并在 `mod.rs` 中实现，待类型膨胀后再拆文件；但公开 API 应按上述边界命名，避免后续迁移影响调用方。

## 7.3 核心类型

### 7.3.1 `Artifact`

```rust
pub struct Artifact {
    pub path: PathBuf,
    pub content: ArtifactContent,
}
```

语义：

- `path` 是最终输出路径，可以是绝对路径，也可以是 build 已按 project root / target output 解析后的路径。
- `content` 统一表达 binary/text 内容；写入层只处理路径、bytes 和写入策略，不理解制品业务来源。

### 7.3.2 `ArtifactContent`

```rust
pub enum ArtifactContent {
    Text(String),
    Binary(Vec<u8>),
}

impl ArtifactContent {
    pub fn into_bytes(self) -> Vec<u8>;
    pub fn len(&self) -> usize;
}
```

Codegen 可以返回 `Text`，runtime data exporter 可以返回 `Binary`。Artifact Writer 不关心文本编码转换之外的业务语义；文本统一以 UTF-8 写入。

### 7.3.3 `ArtifactWriteMode`

```rust
pub enum ArtifactWriteMode {
    DryRun,
    Direct,
    TempCommit,
}
```

语义：

- `DryRun`：检查路径冲突并返回写入计划，不创建目录、不写临时文件、不覆盖最终文件。
- `Direct`：创建父目录并直接写最终路径。
- `TempCommit`：先写入同目录临时文件，全部成功后再 rename/commit；失败时清理已创建的临时文件，未成功提交的 artifact 不写入最终路径。

### 7.3.4 `ArtifactWriteResult`

```rust
pub struct ArtifactWriteResult {
    pub mode: ArtifactWriteMode,
    pub entries: Vec<ArtifactWriteEntry>,
}
```

`entries` 必须按规范化后的 path 稳定排序，便于 CLI 输出和测试断言。Artifact Writer 只返回写入计划和写入结果，不生成业务产物。

## 7.4 Artifact Writer 流程

```text
Build collects Vec<Artifact>
  -> ArtifactWriter::new(mode)
  -> normalize and sort artifact paths
  -> ensure no duplicate final path
  -> if DryRun: return ArtifactWriteResult without filesystem writes
  -> if Direct: create parent dirs and write bytes
  -> if TempCommit: write same-directory temp files, then rename to final paths
```

路径冲突检查必须在任何写入前完成，错误信息包含冲突路径，方便用户定位 data/code task 配置问题。

## 7.5 与 Exporter 的关系

Exporter 的职责从“序列化并写文件”收敛为“序列化并返回 runtime data artifacts”。建议 API 演进：

```rust
pub fn export_runtime_data_artifacts(...) -> Result<Vec<RuntimeDataArtifact>>;

pub struct RuntimeDataArtifact {
    pub file: RuntimeDataFile,
    pub artifact: Artifact,
}
```

其中：

- `RuntimeDataFile` 仍是 codegen 需要消费的 runtime loader 路径契约。
- `Artifact.path` 是实际写入路径，由 data task output root + runtime data file path 得到。
- Exporter 不再调用 `fs::create_dir_all` / `fs::write`。

`export_runtime_data()` 包装函数应通过 Artifact Writer 写出产物，避免存在两套写入路径。

## 7.6 与 Codegen 的关系

Codegen 继续返回生成文件列表，但文件模型应迁移到 artifact 模块或可无损转换为 artifact：

```rust
pub struct GeneratedFile {
    pub path: PathBuf,
    pub content: String,
}
```

后续可直接替换为：

```rust
pub type GeneratedFile = Artifact;
```

第一阶段推荐保留 `GeneratedFile` 作为 codegen 语义类型，Build 在写入前统一转换为 `ArtifactContent::Text`。

## 7.7 与 Build / CLI 的关系

Build 是唯一编排者：

1. CLI 解析 `--dry-run`，写入 `BuildOptions`。
2. Build 正常执行 project/meta/data/validation/exporter/codegen。
3. Build 收集 runtime data artifacts 与 generated code artifacts。
4. Build 调用 `ArtifactWriter`。
5. Dry-run 模式下 Build 返回成功和计划结果，但不写文件。

CLI dry-run 不应跳过 validation 或 serialization/codegen，因为用户需要确认完整构建能成功，只是不提交文件系统变更。

## 7.8 错误边界

第一阶段可复用 `ConfigForgeError`：

| 场景 | 错误 |
| --- | --- |
| 重复 artifact path | `ConfigForgeError::Validation` |
| 创建目录失败 | `ConfigForgeError::WriteFile` |
| 写最终文件失败 | `ConfigForgeError::WriteFile` |
| temp commit rename 失败 | `ConfigForgeError::WriteFile` |

后续如果 artifact 诊断增多，可新增 `ArtifactDiagnostic`，但不应让 exporter/codegen 为写入错误各自包装一套错误类型。

## 7.9 测试计划

Artifact 模块单元测试应覆盖：

1. 相同 path 的 artifact 在写入前报错。
2. dry-run 不创建父目录、不写文件，但返回稳定排序 entries。
3. direct 模式创建父目录并写入 text/binary 内容。
4. write entries 按 path 稳定排序。
5. runtime data artifact 与 generated code artifact 冲突时由统一 writer 报错。
6. temp-commit 模式先写临时文件再提交最终路径，写临时文件失败时清理临时文件。

集成测试应覆盖：

1. CLI `--dry-run` 对标准工程执行成功但不产生输出文件。
2. 正常 build 仍产生 JSON / MessagePack / generated code。
3. data output 与 code output 指向同一路径时报统一 artifact conflict。
4. CLI `--write-mode temp-commit` 产生与 direct 模式等价的最终 artifact。

## 7.10 分阶段落地

### Phase 1：文档与边界

- 新增本文档。
- 调整 Implementation 目录，把 Artifact 作为模块文档插入 Exporter / Codegen / Build 之前。
- 修正模块依赖图和文档索引。

### Phase 2：最小 Artifact 模块

- 新增 `src/artifact/`。
- 实现 `Artifact`、`ArtifactContent`、`ArtifactWriter`、`ArtifactWriteMode::DryRun/Direct`。
- Build generated code 写入改走 Artifact Writer。

### Phase 3：Runtime data 迁移

- Exporter 返回 runtime data artifacts，不再直接写文件。
- Build 统一收集 runtime data + generated code，再一次性调用 Artifact Writer。
- 删除 build/exporter 中重复路径冲突检查或迁移到 artifact writer。

### Phase 4：CLI dry-run 与 temp-commit

- CLI 增加 `--dry-run`。
- `BuildOptions` 增加 artifact write mode / dry-run 标志。
- 实现 direct 与 dry-run 集成测试。
- 后续补齐 temp-commit 原子提交。
