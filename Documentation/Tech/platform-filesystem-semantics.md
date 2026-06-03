# 平台文件系统语义与路径处理策略

本文档记录 ConfigForge 在不同平台文件系统上处理 path、大小写、symlink 与写入冲突时需要遵守的工程策略。它补充 `Implementation/03-project-module-detail-design.md`、`Implementation/07-artifact-module-detail-design.md` 和 `Tech/task-system-design.md`，不替代各模块文档中的职责划分。

## 1. 当前状态：lexical path level

当前实现中，Project path normalization、runtime planned output conflict check 和 Artifact Writer final conflict check 都使用 `lexical_normalize_path()` 这一类词法归一化 key：

- 折叠 `.`；
- 在不越过 root / prefix 的前提下折叠可安全处理的 `..`；
- 不要求目标路径存在；
- 不访问文件系统；
- 不解析 symlink；
- 不按平台或卷的大小写敏感性折叠 path component。

这保证 dry-run、未创建输出目录、首次构建等场景都可以稳定规划 artifact path，但它不是 filesystem-aware conflict detection。

## 2. 平台差异

| 平台 / 文件系统 | 常见大小写语义 | Symlink 支持 | 主要风险 |
| --- | --- | --- | --- |
| macOS APFS / HFS+ 默认卷 | 通常大小写不敏感、大小写保留；也可创建大小写敏感卷 | 支持 symlink；权限和 sandbox 可能影响解析 | `Out/items.json` 与 `out/items.json` 可能落到同一路径；symlink 输出根可绕过 lexical conflict check。 |
| Linux ext4 / XFS 等 | 通常大小写敏感 | 支持 symlink | case alias 通常不是冲突；symlink alias 仍可能让两个 lexical path 指向同一位置。 |
| Windows NTFS / ReFS 默认配置 | 通常大小写不敏感、大小写保留；可按目录启用大小写敏感 | 支持 symlink / junction / mount point，但创建 symlink 受权限和开发者模式影响 | case alias、junction / symlink / 8.3 short name 等可能造成实际同址；路径 prefix、盘符大小写和 UNC 也需要统一处理。 |
| 网络盘 / 虚拟文件系统 | 取决于服务端和 mount 配置 | 取决于服务端和客户端 | 不能只按 OS 推断大小写语义；同一平台不同 mount 可能行为不同。 |

## 3. 风险边界

当前 lexical conflict detection 可以发现以下问题：

- `out/./items.json` 与 `out/items.json`；
- `out/a/../items.json` 与 `out/items.json`；
- 两个 export task 规划出完全相同的 lexical-normalized path。

当前不能保证提前发现以下问题：

- 大小写不敏感卷上的 `Out/items.json` 与 `out/items.json`；
- `out_link/items.json` 与 `out/items.json`，其中 `out_link -> out`；
- Windows junction / mount point alias；
- 网络盘或虚拟文件系统暴露的特殊 alias。

最终写入仍可能被操作系统拒绝、覆盖或表现为同一路径。后续增强的目标是在写入前更早、更稳定地诊断这些问题，而不是依赖 write error 或 platform-specific 覆盖行为。

## 4. 中期 path conflict key 方案

建议新增统一的 artifact/path key builder，供 build/runtime 的早期 planned path check 与 Artifact Writer final check 共用，避免各层实现不同规则。

建议算法：

1. 先计算 lexical-normalized path，作为稳定 fallback key 和用户可读诊断路径。
2. 拆分为已存在父目录与尚不存在 suffix：
   - 从 artifact final path 向上查找第一个存在的 ancestor；
   - 对存在 ancestor 调用 canonicalize，解析 symlink / junction / mount alias；
   - 对不存在 suffix 保留 lexical component，不能要求输出目录预先存在。
3. 组合 `canonicalized_existing_ancestor + lexical_suffix` 得到 filesystem-aware key。
4. 在 macOS / Windows，或探测到所在卷大小写不敏感时，对 key 做 case-fold：
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
- Unicode case-fold 存在 locale 和 normalization 问题；首版可记录为 best-effort，并优先覆盖 ASCII case alias，因为 exporter/codegen 产物文件名通常来自 ASCII table / type / language 命名。

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

- 设计并实现统一 `ArtifactPathKey` / `OutputPathConflictKey` helper。
- 决定大小写敏感性探测方式：平台启发式、目录探测、还是卷 metadata。
- 决定 canonicalize 失败时是否输出 warning，以及 warning 是否进入 `BuildOutput.warnings`。
- 将 build/runtime 早期 planned path conflict check 和 artifact final check 收敛到同一 key builder。
