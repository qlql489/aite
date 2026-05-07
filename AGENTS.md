# Release Notes 规则

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

说明：
- 尽量用 PR 合并，不要只靠直接 push commit。
- 每个 PR 至少打 1 个类型 label，发布说明才会更清晰。
