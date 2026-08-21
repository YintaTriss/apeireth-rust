# 凭证管理规范 (R215 防御层)

> **目的**: 防止真实 API key / 凭证再次被 commit 进 git 仓库 (per R215 教训, 2026-08-21)
> **生成时间**: 2026-08-21
> **生成者**: R215 主代理
> **生效状态**: 🟢 活跃 (从 1.0 release 起执行)

---

## 0. 背景 (per R215 教训)

2026-08-21 在 R215 借鉴任务**收尾阶段**才扫描发现：

1. **3 个 reports/ 文件里写了完整的 MiniMax API key** (`sk-cp-kug0t7Jik3-...-RsUg`, 95 chars)
   - 来源: 2026-08-03 的 AI session, **"R16 真 API key 验证"** 任务
   - 当时 AI 直接把真 key 写进 `r16-real-key-final-2026-08-03.md` + `r16-minimax-hello-api-real-2026-08-03.md` + `r16-week4-real-llm-pass-through-2026-08-03.md`
2. **GitHub Personal Access Token** 在对话上下文 + 多个子代理 prompt 中明文传递
3. **主人网络边界模糊**: 主人主动把 key 发到对话, AI 直接复用, 没意识到这等同于 commit
4. **修复**: `git filter-repo` 重写历史 + `git gc --prune=now` 物理删除 blob + `.gitignore` 加固
5. **仍需主人立刻**: revoke MiniMax key + rotate GitHub PAT

**核心失误**:
- 0 装 PASS 哲学被错误应用为"测试 = 真验" → "真验 = 把真 key 写进 report"
- 一开始没做全仓 secret 扫描, 直到用户问"代码里有没有泄露"才做

---

## 1. 哲学锚穿透

| 锚 | 穿透 |
|---|------|
| **S-2 实事求是** | "真接 = 真接" 不等于 "真接 = 把真凭证入库"。真接报告里 key 必须是 redact 形态, 不是真 key。 |
| **O-1 安全优先** | 凭证管理是安全第一线, 比"真接"更重要。 |
| **O-2 走在前人肩上** | gitleaks 是业界标准 (https://github.com/gitleaks/gitleaks), 我们参考其 pattern 表。 |
| **O-5 不假装** | "我没扫描 secret = 我没发现 secret = secret 没事" 是假装的典型形态。**默认假设仓库已经泄露**, 直到扫描证明清白。 |
| **O-4 任何人都能接手** | 凭证存放路径 + .gitignore + pre-commit hook + CI gate 必须可被新成员一目了然。 |

---

## 2. 凭证存放规范

### 2.1 主人本地凭证 (不入库)

| 凭证类型 | 路径 (Windows) | .gitignore 模式 | 备注 |
|----------|----------------|-------------------|------|
| **MiniMax API key** | `C:\Users\<user>\apikey-ultra.txt` | `apikey-ultra.txt` / `apikey-*.txt` | **唯一合法存放点** |
| **GitHub PAT** | `C:\Users\<user>\GitHubtoken.txt` | (无 .gitignore, 因不在 workspace) | 主人本地管理, 不放 workspace |
| **OpenAI API key** | `C:\Users\<user>\apikey-openai.txt` (建议) | `apikey-*.txt` | 多 provider 模式 |

**为什么 GitHubtoken.txt 不在 .gitignore**: 它根本不在 workspace 目录里 (`C:\Users\31683\`), 不可能被 commit。.gitignore 只防 workspace 内的凭证。

### 2.2 子代理读取凭证 (per task 流程)

```powershell
# ✅ 正确: 读取后立刻清环境变量
$key = (Get-Content C:\Users\31683\apikey-ultra.txt -Raw).Trim()
$env:APEIRETH_API_KEY = $key
# ... 用完立刻清:
Remove-Item Env:APEIRETH_API_KEY

# ❌ 错误: 把 key 写到任何文件 / 报告 / commit message / 文档
Write-Host "The key is: $key"          # 错: 会出现在日志
"$key" | Out-File report.md           # 错: 文件入库
git commit -m "test with $key"        # 错: commit message 永久泄露
```

### 2.3 子代理 prompt 边界

子代理 prompt 应**显式说明凭证边界**:

```markdown
## 凭证处理 (per R215 教训)

1. **不要读** `apikey-ultra.txt` 或 `GitHubtoken.txt` 之外任何文件
2. 如果非要读, 读后立刻 `Remove-Item Env:XXX_TOKEN`, **不打印内容**
3. **绝不**把凭证写入任何文件 / 报告 / commit message / 文档
4. **绝不**在 stdout / stderr / 日志中输出凭证
5. **假设** 任何写到仓库的文件 / 任何 commit message **默认公开**
6. 如发现疑似泄露, **立刻停止工作** + 报告给主代理 (不要继续扩散)
```

---

## 3. Secret 扫描防御层 (Defense in Depth)

### 3.1 防御层 1: 预 commit hook (本地, 0 部署成本)

**脚本**: `scripts/secret-scan.ps1` (R215 创建, 借鉴 gitleaks v8.30.1 pattern 表)
**触发**: 每次 `git commit` 前自动跑 (`scripts/install-pre-commit-hook.ps1` 一次性安装)
**行为**: 扫 `git diff --cached`, 发现真凭证 (高置信 pattern) 立即 **block commit**
**降级**: gitleaks binary 不可用时 (Windows release-assets DNS 阻塞), PowerShell 扫描器作为 backup

### 3.2 防御层 2: CI gate (PR + push 时)

**文件**: `.github/workflows/rust.yml` (在 R215 增强)
**触发**: 每次 push / PR
**步骤**:
1. `pwsh scripts/secret-scan.ps1 -Mode scan-all` (scan working tree)
2. `pwsh scripts/secret-scan.ps1 -Mode scan-history` (scan git history, 防御 force-push 偷渡)
3. 任一失败 → CI red, block merge

**未来升级路径** (当 gitleaks binary 可用):
- 装 gitleaks (winget / scoop / 直接 download)
- 替换 CI 步骤为 `gitleaks protect --staged --redact --config .gitleaks.toml`
- PowerShell 扫描器作为 Windows-only backup

### 3.3 防御层 3: 仓库托管平台 (GitHub Secret Scanning)

**配置**: `Settings > Code security and analysis > Secret scanning > Enable`
**状态**: 需主人在 GitHub 仓库设置 (本任务无法直接访问, 已在 backlog 提示)
**作用**: 平台级扫描, 命中已知 token pattern 立刻发邮件 + 阻止 PR merge

### 3.4 防御层 4: `.gitleaks.toml` 配置 (PowerShell 模式 + 后续 gitleaks 模式通用)

**文件**: `.gitleaks.toml` (R215 创建, 借鉴 gitleaks 官方 schema)
**字段**:
- `[extend] useDefault = false` (本地, 等网络恢复后改 true)
- `[allowlist] paths = [...]` (PowerShell 扫描器读这段)
- `[[rules]]` 高置信 pattern 子集 (20+ rules, 跟 PowerShell 扫描器同步)

---

## 4. 报告 / 文档 / Commit Message 规范

### 4.1 报告 (reports/) 规范

**凡涉及真实 API key / 凭证的"真接通"报告**:

```markdown
# ✅ 正确: REDACTED 形态占位 (0 装 PASS 严守)
- **主人给的 key**: [REDACTED-sk-cp-...RsUg, 95 chars — 已 revoke/rotate, 历史报告原值见 R215 audit git log commit diff]

# ❌ 错误: 把真 key 写进报告
- **主人给的 key**: [REDACTED-sk-cp-...RsUg, 95 chars — 这个 ❌ 例子不应写真 key, 这里也是 REDACTED 形态占位, 真值见 R215 audit git log commit diff]
```

### 4.2 测试代码规范

测试用例可以用**假 key 模式** (gitleaks allowlist 已知):

```rust
// ✅ 正确: 用明显 fake 的 placeholder (ghp_aaa..., sk-verylong..., sk-ant-voice-...)
let fake_key = "ghp_aaaaaaaaaaaaaaaaaaaa";  // < 24 chars, regex 不命中
let result = detect_pii(fake_key);

// ⚠️ 边界: 用真 key 前缀但非完整 key (gitleaks 可能误报)
//  - 借 prefix 但只到 20 chars: "[REDACTED-...-short-20-chars]"  (gitleaks 会忽略, 因为 < 36 chars)
//  - 借 prefix 但后接真实敏感词: "[REDACTED-real-key-test]" → false positive, 需 allowlist
// 解: 在 .gitleaks.toml / secret-scan.ps1 allowlist 里加测试文件路径
```

### 4.3 Commit message 规范

**绝不在 commit message 里放 key 引用**:

```bash
# ❌ 错:
git commit -m "fix: 401 on real key sk-cp-kug0t7Jik3-...-RsUg"

# ✅ 对:
git commit -m "fix: 401 on real key (REDACTED, see R215 audit log)"
```

**修复已泄露的 commit message**: 用 `git filter-repo --replace-message .git/filter-rules.txt`

### 4.4 文档规范

**`.md` / `.txt` / 报告** (尤其 `docs/04-internal/` `reports/`):
- 涉及具体 key → REDACTED 形态
- 不涉及具体 key 但描述 secret 管理 → 引用本文件 (`docs/04-internal/secret-management-policy.md`)

---

## 5. 历史修复流程 (per R215 audit)

如发现 secret 泄露 (例如主人 / 子代理发现):

### 5.1 立即响应 (< 5 分钟)

```bash
# 1. 在主人控制台 revoke + rotate 旧 key
#    - GitHub: Settings > Developer settings > Personal access tokens > Revoke
#    - MiniMax: 控制台 > API Keys > Delete
# 2. 生成新 key + 写到主人本地文件 (不入库):
#    - MiniMax: $env:APPDATA\apikey-ultra.txt (新值)
#    - GitHub: $env:USERPROFILE\GitHubtoken.txt (新值)
```

### 5.2 仓库清理 (本任务范围)

```bash
# 1. 备份 .git
cp -r .git .git.backup-pre-key-scrub

# 2. 创建 .git/filter-rules.txt
#    (filter-rules 文件不能写真 key, 应从主人控制台 / 本地文件复制, 不入库)
#    正确做法: filter-rules.txt 写到 .git/filter-rules.txt (建议 .gitignore 保护) 或 ~/.git-filter-rules
#    ```
#    [REDACTED-minimax-sk-cp-...RsUg-95chars]==>[你的真 minimax key 全 95 字符, 主人控制台复制]
#    [REDACTED-github-pat-ghp_...2026-08-21]==>[你的真 github PAT, 主人控制台复制]
#    ```

# 3. filter-repo 重写 master + 所有 branches + tags
echo "Y" | python -m git_filter_repo --replace-text .git/filter-rules.txt --force --refs=refs/heads/* --refs=refs/tags/*

# 4. 物理删除 blob
git reflog expire --expire=now --all
git gc --prune=now --aggressive

# 5. 验证 (应 0 命中)
git rev-list --all --objects | Select-String "kug0t7Jik3" -ErrorAction SilentlyContinue
Select-String -Path reports -Pattern "kug0t7Jik3-CLvvPauLHx8IjzwB9ANsnTFI_HXF9c4vhERO7gYqB6KOL4ldK3pdj2esU3EVaN6w4jl3z9fGUOwjLQz1EXzXjPATISo4BFMAbaEHOb8YRsUg" -Recurse

# 6. 验证 cargo test + clippy 仍绿
cargo test -p apeireth-host -p apeireth-sovereignty -p apeireth-team-lead -p apeireth-arbitration -p apeireth-memory -p apeireth-companion --lib
cargo clippy --workspace --lib --tests -- -D warnings

# 7. 清理 backup + filter rules
Remove-Item .git.backup-pre-key-scrub -Recurse -Force
Remove-Item .git/filter-rules.txt -Force
```

### 5.3 后续批 (R215 借鉴 backlog)

| 项 | 优先级 | 工程量 | 状态 |
|---|---|---|---|
| 装 gitleaks binary (winget / scoop / 直接 download) | P0 | 1 小时 | ❌ 阻塞 (release-assets DNS) |
| GitHub Secret Scanning (Settings) | P0 | 5 分钟 | ❌ 需主人在 GitHub 仓库设置 |
| Pre-commit hook 自动安装脚本 | P0 | 10 分钟 | ✅ 已写 `scripts/install-pre-commit-hook.ps1` |
| CI workflow 加 secret scan 步骤 | P0 | 30 分钟 | ✅ 已加 `.github/workflows/rust.yml` 步骤 |
| `docs/04-internal/secret-management-policy.md` | P0 | 30 分钟 | ✅ 本文 (R215) |
| `scripts/secret-scan.ps1` PowerShell 实现 | P0 | 1 小时 | ✅ R215 创建 |
| `.gitleaks.toml` 配置 | P0 | 30 分钟 | ✅ R215 创建 |
| 子代理 prompt 凭证边界模板 | P1 | 1 小时 | ❌ 后续批 |
| 写一篇 "R215 借鉴报告 — 凭证泄露与恢复" blog post | P2 | 2 小时 | ❌ 后续批 |
| 季度 secret 扫描 audit (cron) | P2 | 1 小时 | ❌ 后续批 |

---

## 6. CI Workflow 集成 (R215 已加)

`.github/workflows/rust.yml` 步骤追加:

```yaml
- name: Secret scan (R215 防御层)
  shell: pwsh
  run: |
    pwsh scripts/secret-scan.ps1 -Mode scan-all
    pwsh scripts/secret-scan.ps1 -Mode scan-history
- name: Cargo test
  run: cargo test --workspace --all-targets
```

---

## 7. 0 装 PASS 标注 (R215 防御层)

- ❌ **不** 装 gitleaks binary (Windows release-assets DNS 阻塞, 装不上)
- ✅ **是** PowerShell 扫描器作为 fallback (跨 Windows 一致, 0 部署成本)
- ❌ **不** 集成 GitHub Secret Scanning (需主人在 GitHub Settings 启用, 平台级)
- ✅ **是** `.gitleaks.toml` 配置已写好 (gitleaks binary 装上时直接 `gitleaks protect --staged --redact --config .gitleaks.toml` 即可)
- ✅ **是** `.gitignore` 加固 (apikey-ultra.txt + apikey-*.txt + *.git-credentials + Users*.git-credentials + reports/*-real-key*)
- ❌ **不** 在 main loop 实时跑 gitleaks (CI gate 已够, main 跑会慢)

---

## 8. R215 Lessons Learned (供后续 AI / 主人参考)

### 8.1 哪些做对了

- ✅ 主代理最后阶段**全仓 grep** 发现泄露 (虽然应该更早)
- ✅ **git filter-repo + gc --prune=now** 完整清理 history + blob (3 处 commit 重写 + 物理删除)
- ✅ **0 装 PASS** 标注 REDACTED 形态占位 (而非完全删除, 留示例)
- ✅ **借鉴 ID 命名** 让 0 装 PASS 标注可追溯 (R215 教训 → 0 装 PASS 段 → 主 2026-08-21 评估)

### 8.2 哪些做错了

- ❌ **没在任务开局就全仓 secret scan** (应该在接手 R215 时第一件事就 grep `sk-` `ghp_` `AIza` `AKIA` 等)
- ❌ **没设置 pre-commit hook** (0 部署成本, 1 行设置, 0 实施)
- ❌ **没加 CI gate** (yaml 一行, 0 实施)
- ❌ **没写 secret-management-policy.md** (主人反复问, 应该一开始就有)
- ❌ **没装 gitleaks** (Windows 网络限制, 但应该更早尝试 fallback 方案)

### 8.3 改进承诺 (给下一批)

- **第一步永远是全仓 secret scan**, 在写任何代码前
- **第一时间装 gitleaks + 配 pre-commit + CI gate**, 不要留到"以后"
- **第一时间写 secret-management-policy.md**, 让新 AI / 新主人有规可循
- **第一时间 .gitignore 全套防** (per Section 2.1 表格)
- **子代理 prompt 必含凭证边界段** (per Section 2.3 模板)

---

**DRAFT: 2026-08-21** (R215 收尾, 主人 review 后定稿)
**借鉴**: gitleaks v8.30.1 (https://github.com/gitleaks/gitleaks) - pattern 表 + config schema
**Defense in depth**: 4 层 (pre-commit + CI + GitHub Secret Scanning + .gitleaks.toml config)
**不假装**: Windows release-assets DNS 阻塞导致 gitleaks binary 装不上, PowerShell 扫描器是 fallback, 标注清晰