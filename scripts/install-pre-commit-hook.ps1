#!/usr/bin/env pwsh
# install-pre-commit-hook.ps1 — 安装 pre-commit secret 扫描 hook
#
# 借鉴 gitleaks pre-commit pattern, 简化版: 用本地 PowerShell 扫描器
# (gitleaks binary 当前装不上, 见 docs/04-internal/secret-management-policy.md)
#
# 用法:
#   pwsh scripts/install-pre-commit-hook.ps1           # 安装
#   pwsh scripts/install-pre-commit-hook.ps1 -Uninstall # 卸载
#
# 行为:
#   1. 创建 .git/hooks/pre-commit
#   2. 内容: 调 pwsh scripts/secret-scan.ps1 -Mode scan-staged
#   3. 扫描失败 → exit 1 → git commit 失败
#   4. 扫描成功 → exit 0 → git commit 继续
#
# 0 装 PASS:
#   - 已安装? 不会重复覆盖 (idempotent)
#   - uninstall 安全 (仅删除我们创建的 hook, 不动其他 hook)
#   - hook 仅在 apeireth-rust workspace 内有效 (脚本会验证 .git 目录)

[CmdletBinding()]
param(
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'

# 验证在 git repo 内
$gitDir = & git rev-parse --git-dir 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Host "[FAIL] Not in a git repository" -ForegroundColor Red
    exit 1
}
$hooksDir = Join-Path $gitDir "hooks"
$hookPath = Join-Path $hooksDir "pre-commit"

if ($Uninstall) {
    if (Test-Path $hookPath) {
        $content = Get-Content $hookPath -Raw
        if ($content -match "secret-scan\.ps1") {
            Remove-Item $hookPath -Force
            Write-Host "[OK] Pre-commit hook uninstalled." -ForegroundColor Green
        } else {
            Write-Host "[SKIP] Existing pre-commit hook is not from secret-scan (left untouched)" -ForegroundColor Yellow
        }
    } else {
        Write-Host "[SKIP] No pre-commit hook to uninstall" -ForegroundColor Yellow
    }
    exit 0
}

# 检查是否已安装
if (Test-Path $hookPath) {
    $content = Get-Content $hookPath -Raw -ErrorAction SilentlyContinue
    if ($content -match "secret-scan\.ps1") {
        Write-Host "[OK] Pre-commit hook already installed (idempotent)" -ForegroundColor Green
        exit 0
    } else {
        Write-Host "[WARN] Existing pre-commit hook (not ours). Backing up to pre-commit.bak" -ForegroundColor Yellow
        Move-Item $hookPath "$hookPath.bak" -Force
    }
}

# 写 hook (幂等 + 显式失败信息)
$repoRoot = & git rev-parse --show-toplevel
$scriptPath = Join-Path $repoRoot "scripts/secret-scan.ps1"

$hookContent = @"
#!/bin/sh
# Pre-commit secret scan (R215 防御层, 见 docs/04-internal/secret-management-policy.md)
# 借鉴 gitleaks pre-commit pattern, 简化: 用 PowerShell 扫描器扫 staged files.
# 不打印 / 不读凭证内容, 仅 0/1 退出码.
set -e
if command -v pwsh >/dev/null 2>&1; then
    pwsh "$scriptPath" -Mode scan-staged
    exit `$?
else
    echo "[secret-scan] pwsh not found in PATH, skipping (manual scan recommended)"
    exit 0
fi
"@

Set-Content -Path $hookPath -Value $hookContent -NoNewline

# chmod +x (Windows: 用 git 的 hook 机制, 实际不需要)
# 但为了一致性, 仍然 chmod
& git update-index --chmod=+x $hookPath 2>$null

Write-Host "[OK] Pre-commit hook installed: $hookPath" -ForegroundColor Green
Write-Host "     Test: try `git commit` with a fake 'sk-test1234' string in staged file" -ForegroundColor Gray
exit 0
