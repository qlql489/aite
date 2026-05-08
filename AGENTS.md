# Release Notes 与 PR 规则

GitHub 自动生成 release notes 时，按以下 PR label 分组：

- `feature` / `enhancement` / `feat` -> `新功能`
- `fix` / `bug` / `bugfix` -> `修复`
- `perf` / `performance` -> `性能优化`
- `refactor` -> `重构`
- `docs` -> `文档`
- `ci` / `build` / `chore` -> `构建与发布`
- `test` / `tests` -> `测试`
- 其他 label -> `其他变更`

不想出现在 release notes 里的 PR，打 `skip-changelog`。

## PR 规范

- 尽量用 PR 合并，不要只靠直接 push commit。
- 每个 PR 至少打 1 个类型 label，发布说明才会更清晰。
- PR 标题建议使用 `<type>: 简短描述` 格式，例如 `feat: 优化更新入口体验`。
- PR 描述必须使用中文。
- PR 描述至少包含以下内容：
  - `## 变更摘要`
  - `## 验证`
- `## 变更摘要` 中应清楚说明本次改动点，便于评审和生成发布说明。
- `## 验证` 中应列出实际执行过的命令或验证方式；如果未验证，需要明确说明。
