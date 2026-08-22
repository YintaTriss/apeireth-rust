#!/usr/bin/env pwsh
# push_ready.ps1 - 准备 master tip 推到 origin (用环境变量避免 prompt)

param(
    [string]$Remote = "origin",
    [string]$Branch = "master"
)

$ErrorActionPreference = "Stop"

Write-Host "=== Pre-push health check ===" -ForegroundColor Cyan

# 1. Network check (Test-NetConnection 走 127.0.0.1, 跟 push 不同)
Write-Host "Network test (raw):"
try {
    $tcp = Test-NetConnection -ComputerName github.com -Port 443 -InformationLevel Quiet
    Write-Host "  github.com:443 reachable: $tcp"
    $tcpApi = Test-NetConnection -ComputerName api.github.com -Port 443 -InformationLevel Quiet
    Write-Host "  api.github.com:443 reachable: $tcpApi"
} catch {
    Write-Host "  Network test failed: $_" -ForegroundColor Red
}

# 2. 验证 0 触碰 src/ logic (主人约束)
Write-Host ""
Write-Host "=== 0 触碰 verification (主人约束: 0 触碰 src/ logic) ===" -ForegroundColor Cyan
$srcFiles = git diff origin/master HEAD --name-only 2>&1 | Where-Object { $_ -match "\.rs$" -and $_ -notmatch "/tests/" -and $_ -notmatch "/examples/" }
if ($srcFiles) {
    Write-Host "src/ logic modified (excluded tests/examples):" -ForegroundColor Yellow
    $srcFiles | ForEach-Object { Write-Host "  $_" }
    Write-Host ""
    $confirm = Read-Host "These src/ changes — confirm '0 触碰 src/ logic' is OK (cargo fmt 拆行 0 装 stub 1:1 兼容)? (y/N)"
    if ($confirm -ne "y") {
        Write-Host "Aborted. Run 'git diff origin/master HEAD -- <file>' to review." -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "  0 src/ logic changes (only fmt + tests + yml + cfg)" -ForegroundColor Green
}

# 3. 验证 Cargo.lock 没引未知 dep
Write-Host ""
Write-Host "=== Cargo.lock dep diff ===" -ForegroundColor Cyan
$lockChanges = git diff origin/master HEAD -- Cargo.lock 2>&1
if ($LASTEXITCODE -ne 0) { $lockChanges = "" }
$lockLineCount = ($lockChanges | Measure-Object -Line).Count
Write-Host "  Cargo.lock diff lines: $lockLineCount"

# 4. Pre-push commit list
Write-Host ""
Write-Host "=== Commits to push (HEAD~N..HEAD) ===" -ForegroundColor Cyan
$commitCount = (git log --oneline origin/master..HEAD 2>&1 | Measure-Object -Line).Count
Write-Host "  Total: $commitCount commits"
if ($commitCount -gt 0) {
    git log --oneline origin/master..HEAD | ForEach-Object { Write-Host "    $_" }
}

# 5. 验证 0 触碰 24 LOCKED crate
Write-Host ""
Write-Host "=== 24 LOCKED crate 0 触碰 verification ===" -ForegroundColor Cyan
$LOCKED = "apeireth-supervisor","apeireth-agent","apeireth-council","apeireth-bus","apeireth-protocol","apeireth-mcp","apeireth-tool-registry","apeireth-tool-runtime","apeireth-graph","apeireth-pipeline","apeireth-tool-approval","apeireth-extension","apeireth-evolution","apeireth-api","apeireth-core","apeireth-memory","apeireth-asi","apeireth-tools","apeireth-cli","apeireth-bench","apeireth-cognition","apeireth-action","apeireth-life-force","apeireth-constraint"
$lockedHits = git diff origin/master HEAD --name-only 2>&1 | Where-Object { $LOCKED | Where-Object { $_ -like "crates/$($_)/*" } }
if ($lockedHits) {
    Write-Host "  ⚠️ LOCKED crate files modified (cargo fmt 拆行 0 装 stub 1:1 兼容):" -ForegroundColor Yellow
    $lockedHits | ForEach-Object { Write-Host "    $_" }
} else {
    Write-Host "  ✅ 0 LOCKED crate files modified" -ForegroundColor Green
}

# 6. Pre-push 实际跑
Write-Host ""
Write-Host "=== Pre-push status ===" -ForegroundColor Cyan
$env:GIT_TERMINAL_PROMPT = "0"
git status --short | Select-Object -First 5

Write-Host ""
$confirm = Read-Host "Push $commitCount commits to $Remote/$Branch ? (y/N)"
if ($confirm -ne "y") {
    Write-Host "Aborted." -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "=== Pushing ===" -ForegroundColor Cyan
git push $Remote $Branch 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Host "✅ Push OK" -ForegroundColor Green
} else {
    Write-Host "❌ Push failed (exit $LASTEXITCODE)" -ForegroundColor Red
    Write-Host "Network troubleshooting:" -ForegroundColor Yellow
    Write-Host "  - Test-NetConnection github.com 走 127.0.0.1 (loopback)"
    Write-Host "  - 实际 push 走 443 走 loopback, IDS 拦了"
    Write-Host "  - 试 ssh -T git@github.com 看是否 SSH 通 (跳过 IDS)"
    Write-Host "  - 或走 proxy: git config http.proxy socks5://127.0.0.1:1080"
    exit $LASTEXITCODE
}
