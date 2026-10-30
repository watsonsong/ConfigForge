# 平台文件系统语义与路径处理策略

本文档记录 ConfigForge 在不同平台文件系统上处理 path、大小写、symlink 与写入冲突时需要遵守的工程策略。它补充 `Implementation/03-project-module-detail-design.md`、`Implementation/07-artifact-module-detail-design.md` 和 `Tech/task-system-design.md`，不替代各模块文档中的职责划分。

## 1. 当前状态：统一产物规划与 filesystem-aware key

当前实现中，Project path normalization 使用 `lexical_normalize_path()` 这一类词法归一化；Artifact Writer 在配置 output root 后会额外做 root containment、已存在 ancestor canonicalize、macOS / Windows case-fold conflict key、Unix hardlink inode key 与 symlink escape 检查：

- 折叠 `.`；
- 在不越过 root / prefix 的前提下折叠可安全处理的 `..`；
- 不要求目标路径存在；
- Project path normalization 不访问文件系统；artifact conflict check 只读取现有 metadata；
- Project config 归一化不解析 symlink；
- Artifact conflict key 对已存在 ancestor 解析 symlink / filesystem alias；
- Artifact conflict key 在 macOS / Windows 采用 ASCII case-fold 的平台启发式。

Build 在 Meta link 后、任何 generator 写入前收集全部路径，并通过共享 `ArtifactSink::reserve_paths()` 检查与预留。该入口与 `create()` 使用同一套 filesystem-aware key，覆盖 MetaOnly 与 MetaAndData 之间的冲突，同时支持 dry-run、未创建输出目录与首次构建。

## 2. 平台差异

| 平台 / 文件系统 | 常见大小写语义 | Symlink 支持 | 主要风险 |
| --- | --- | --- | --- |
| macOS APFS / HFS+ 默认卷 | 通常大小写不敏感、大小写保留；也可创建大小写敏感卷 | 支持 symlink；权限和 sandbox 可能影响解析 | `Out/items.json` 与 `out/items.json` 可能落到同一路径；symlink 输出根可绕过 lexical conflict check。 |
| Linux ext4 / XFS 等 | 通常大小写敏感 | 支持 symlink | case alias 通常不是冲突；symlink alias 仍可能让两个 lexical path 指向同一位置。 |
| Windows NTFS / ReFS 默认配置 | 通常大小写不敏感、大小写保留；可按目录启用大小写敏感 | 支持 symlink / junction / mount point，但创建 symlink 受权限和开发者模式影响 | case alias、junction / symlink / 8.3 short name 等可能造成实际同址；路径 prefix、盘符大小写和 UNC 也需要统一处理。 |
| 网络盘 / 虚拟文件系统 | 取决于服务端和 mount 配置 | 取决于服务端和客户端 | 不能只按 OS 推断大小写语义；同一平台不同 mount 可能行为不同。 |

## 3. 风险边界

当前统一 conflict detection 可以在写入前发现以下问题：

- `out/./items.json` 与 `out/items.json`；
- `out/a/../items.json` 与 `out/items.json`；
- 两个 export task（包括跨 MetaOnly / MetaAndData 分支）规划出相同的 normalized path；
- 已存在 ancestor 可解析的 symlink / junction alias、macOS / Windows ASCII case alias，以及 Unix 已存在文件的 hardlink alias。

当前仍不能保证提前发现以下问题：

- canonicalize 因权限或特殊文件系统行为失败时的 junction / mount point alias；
- 网络盘或虚拟文件系统暴露的特殊 alias。

最终写入仍可能被操作系统拒绝、覆盖或表现为同一路径。后续增强的目标是在写入前更早、更稳定地诊断这些问题，而不是依赖 write error 或 platform-specific 覆盖行为。

## 4. 当前 path conflict key 规则

artifact 模块提供统一的 path normalization 与 conflict key builder，由 sink 的全局预留和创建流阶段共用；Build 不再维护独立的 lexical planned path check。

当前算法：

1. 先计算 lexical-normalized path，作为稳定 fallback key 和用户可读诊断路径。
2. 拆分为已存在父目录与尚不存在 suffix：
   - 从 artifact final path 向上查找第一个存在的 ancestor；
   - 对存在 ancestor 调用 canonicalize，解析 symlink / junction / mount alias；
   - 对不存在 suffix 保留 lexical component，不能要求输出目录预先存在。
3. 组合 `canonicalized_existing_ancestor + lexical_suffix` 得到 filesystem-aware key。
4. 在 macOS / Windows 使用 ASCII case-fold 的平台启发式；Unix 已存在文件优先使用 inode identity key：
   - case-fold 只用于 conflict key；
   - 写入路径和 dry-run 输出仍保留用户配置的原始大小写 / lexical-normalized 展示路径。
5. 如果 canonicalize 因权限、竞态删除、网络盘错误等失败：
   - 不应在 dry-run 阶段直接阻断所有构建；
   - 回退到 lexical key，并可在 verbose / warning 诊断中提示 conflict detection precision 降级。

不存在路径必须保留 lexical fallback。ConfigForge 不应为了提升冲突检查精度而要求用户预先创建 output 目录。

## 5. 大小写折叠策略

大小写折叠应谨慎处理：

- 不要改变实际写入路径，只改变 conflict key。
- 不要全平台无条件 case-fold，否则 Linux 大小写敏感文件系统上合法的 `Items.json` / `items.json` 会被误报冲突。
- macOS / Windows 可以先采用平台默认启发式作为中期方案，但长期最好按 path 所在卷 / 目录探测大小写敏感性。
- Unicode case-fold 存在 locale 和 normalization 问题；首版可记录为 best-effort，并优先覆盖 ASCII case alias，因为 artifact generator 产物文件名通常来自 ASCII table / type / language 命名。

## 6. Symlink / junction 策略

Symlink 与 junction 处理目标是“同一实际父目录下的同名 artifact 不能被两个 lexical path 绕过”：

- 对已存在父目录 canonicalize，解析 symlink / junction。
- 对不存在 suffix 不 canonicalize，也不创建目录探测。
- TempCommit 需要保证临时文件与最终文件位于同一 canonicalized parent，避免跨文件系统 rename 失败；若 parent 是 symlink，应以最终写入 API 看到的路径创建 temp，但 conflict key 用 canonical parent。
- 如果两个 output root 分别是 symlink 与真实目录，且计划写同一 suffix，应在写入前报 path conflict。

## 7. 与 dry-run / 并发写入的关系

Dry-run 必须执行与真实写入一致的 artifact planning 和 conflict check，但不能创建目录或临时文件。因此 filesystem-aware key builder 只能读取现有 filesystem metadata，不能通过创建 probe file / directory 来判断行为。

并发写入前仍必须先完成全局 conflict check。引入 canonicalize 和 case-fold 后，key builder 应在 coordination 阶段完成，不能散落到 worker 写入任务中；否则错误顺序和部分写入风险会变得不可控。

## 7.1 TempCommit replace 语义

`direct` 与 `temp-commit` 对已有最终 artifact 的产品语义应一致：新 artifact 覆盖旧文件。平台 API 行为不同：Unix / macOS 上 `rename(temp, final)` 通常可以替换已有普通文件；Windows 上 `std::fs::rename` 遇到已有目标通常失败。因此 ArtifactSink 的 TempCommit 提交不能只依赖裸 `rename` 的平台默认行为：Windows 路径应显式删除已有 final 后再 rename temp，以维持“覆盖已有 artifact”的用户语义。

该策略只解决单个 final path 的跨平台覆盖一致性，不把 TempCommit 提升为完整多文件事务。提交阶段如果已经删除旧 final、但 rename temp 失败，旧文件可能已经不可恢复；错误仍按 `WriteFile` 报告，并由用户重新运行构建恢复一致状态。

## 8. 测试建议

测试分层建议：

- 纯 lexical 单元测试：继续覆盖 `.` / `..`、absolute root、leading parent 等平台无关逻辑。
- Artifact key 单元测试：用临时目录和 symlink（在支持的平台上）验证 `out` 与 `out_link` 指向同一 key。
- macOS / Windows case-insensitive 测试：在默认大小写不敏感环境下验证 `Out/items.json` 与 `out/items.json` 报冲突；在大小写敏感环境中允许跳过或根据探测结果调整断言。
- Windows junction / symlink 测试：受权限影响，应允许 graceful skip。
- Dry-run 测试：确保不存在 output 目录时仍只使用 lexical fallback，不要求提前创建目录。

## 9. 当前开放事项

- 决定大小写敏感性探测方式：当前 Artifact Writer 使用平台启发式，长期可升级为目录探测或卷 metadata。
- 决定 canonicalize 失败时是否输出 warning，以及 warning 是否进入 `BuildOutput.warnings`。
