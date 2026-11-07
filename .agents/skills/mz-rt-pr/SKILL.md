---
name: mz-rt-pr
description: >-
  Use this skill when the user invokes /mz-rt-pr or asks to "开 PR", "准备发版",
  "推送远程开 PR", or "发布新版本" in the Mazbri repository. It packages local commits
  (typically accumulated on local main) onto a clean feature/fix branch, resets
  local main to origin/main to prevent divergence after GitHub Squash/Release-Please
  merges, pushes to origin, and opens a Release-Please-compatible Pull Request.
---

# Mazbri 远程推送与开 PR 发版流 (`mz-rt-pr`)

本 Skill 专为 Mazbri 仓库的单人迭代开发习惯（平时直接在本地 `main` 逐步提交，全部开发验证完毕后统一推送远端开 PR 触发 CI 与 Release Please 自动发版）设计。

> [!IMPORTANT]
> **执行原则**：
> 1. **禁止在执行本 Skill 时重复运行本地慢速 CI（`pnpm build` / `cargo clippy` / `cargo test`）**，重型编译与测试统一交由云端 GitHub Actions PR CI 执行。
> 2. **严禁手动修改版本号相关文件**（`VERSION`、`.release-please-manifest.json`、`CHANGELOG.md`），版本号与 Changelog 必须完全由第二次 PR 的 Release Please Bot 自动计算并写入。

---

## 标准执行步骤

### Step 1: 远端同步与轻量预检（秒级）

执行以下命令检查当前分支、工作区状态及领先 `origin/main` 的提交列表：

```bash
git fetch origin
git status -s
git branch -vv
git log --oneline origin/main..HEAD
```

1. **未提交改动与远端基线对齐检查**：
   - 若工作区存在未提交的业务代码改动，先提醒用户或按中文 Commit 规范完成本地提交；若存在临时缓存目录（如 `__pycache__`），先清理干净。
   - 检查 `origin/main` 是否已包含上一轮 GitHub Squash 合并或 `chore(main): release x.y.z` 提交（即 `git rev-list --count HEAD..origin/main` > 0）：若大于 0，先将本次新增的本地提交变基到最新 `origin/main` 之上（`git rebase origin/main`），确保分支基于最新远端主干且不含已合入的历史零散提交。
2. **发版元数据零改动检查**：
   - 检查 `git diff --name-only origin/main..HEAD` 中是否包含 `VERSION`、`.release-please-manifest.json` 或 `CHANGELOG.md`。
   - 若被手动修改，必须提醒并还原（这些文件只能由 Release Please Bot 在第二个 PR 中修改）。
3. **Conventional Commits 前缀检查（确保触发 Release Please Bot）**：
   - Release Please 默认只识别 `feat` 和 `fix`（忽略 `refactor`、`docs`、`chore`）。
   - 如果 `origin/main..HEAD` 的提交中没有任何 `feat(...)` 或 `fix(...)`（全为 `refactor:` / `docs:` 等），且用户可能使用普通 Merge Commit 合入，需将最新一条功能性提交（或 PR 标题）确保以 `feat(<scope>):` 或 `fix(<scope>):` 开头；若最新提交前缀不规范（如 `add:`），使用 `git commit --amend` 修正为标准前缀（`feat:` / `fix:` / `refactor:` / `docs:`）。

---

### Step 2: 剥离本地 `main` 提交至功能分支并归位本地 `main`

为避免用户在 GitHub 点击 **Squash and merge** 及合并 **Release Please PR** 后导致本地 `main` 与 `origin/main` 历史分叉（Diverged）：

1. **生成语义化分支名（不带版本号）**：
   - 根据本次 `origin/main..HEAD` 的主要改动范畴，命名为 `feat/<topic>` 或 `fix/<topic>`（例如 `feat/ops-ui-governance`、`fix/gateway-timeout`），**不要在分支名后手写版本号**。
2. **分支移花接木**：
   - **情况 A：当前处于本地 `main` 分支（默认开发流）**
     ```bash
     # 1. 从当前 HEAD 切出新功能分支，完整承载本地攒下的所有提交
     git checkout -b <feat-or-fix>/<topic>
     # 2. 将本地 main 分支游标无损退回 origin/main 基线，确保合并后可丝滑 fast-forward
     git branch -f main origin/main
     # 3. 确保本地 main 的 upstream 仍指向 origin/main
     git branch --set-upstream-to=origin/main main
     ```
   - **情况 B：当前已处于非 `main` 的功能分支**
     - 直接沿用当前分支，无需重置 `main`。

---

### Step 3: 推送功能分支并创建第一次 PR (`gh pr create`)

1. **推送到远端 `origin`**：
   ```bash
   git push -u origin HEAD
   ```
2. **创建 Pull Request**：
   - **PR 标题铁律**：`--title` **必须**以 `feat(<scope>): <中文简述>` 或 `fix(<scope>): <中文简述>` 开头（严禁使用 `refactor:` / `chore:` 作为 PR 标题，否则 Squash 合并后 Release Please 不会创建发版 PR），且**不要在标题中写版本号**。
   - **PR 正文**：按模块分类汇总 `origin/main..HEAD` 的核心改动点。
   ```bash
   gh pr create --base main --head "<branch_name>" --title "<feat|fix>(<scope>): <中文标题>" --body "<结构化中文变更说明>"
   ```

---

### Step 4: 切回本地 `main` 并输出两步手动点击指引

1. **切回干净的本地 `main` 分支（可选，若工作区干净）**：
   ```bash
   git checkout main
   ```
   > 此时本地 `main` 与 `origin/main` 完全对齐；如果后续需要在该 PR 上追加修复提交，只需 `git checkout <branch_name>` 提交并 `git push` 即可。

2. **向用户输出 PR 链接与两步点击发版指引**：
   - **PR 链接**：附上刚创建的 GitHub PR URL。
   - **第一步手动点击（合并功能 PR）**：
     - 等待该 PR 的 GitHub Actions CI（`CI result`）变绿后，在页面点击 **Squash and merge** 或 **Merge pull request**（确保合并 Commit 标题保持 `feat(...)` / `fix(...)` 开头，且末尾保留 `(#PR号)`）。
     - 合入 `main` 后，主干 CI 的 `Dedupe heavy jobs on main push after green PR` 会自动识别该 PR 已绿并**跳过重型编译测试**；同时 `Release Please` 工作流会自动开启第二个 PR（`chore(main): release x.y.z`）。
   - **第二步手动点击（合并 Bot 发版 PR 触发正式发布）**：
     - 前往自动生成的 `chore(main): release x.y.z` PR 页面点击合并。
     - 合并后将自动创建 `vx.y.z` Git Tag 与 GitHub Release，并触发 `release.yml` 编译推送 11 个微服务的 `amd64` + `arm64` 双架构镜像。
   - **发版后本地同步提示**：
     - 两步合并完成后，下次本地开发前只需在 `main` 执行 `git pull --ff-only` 即可无冲突同步至最新发版基线。
