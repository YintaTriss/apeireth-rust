param([int]$PrNumber = 0)
# Privacy: 0 hardcode GitHub PAT in scripts. Read from $env:GITHUB_TOKEN
# Set $env:GITHUB_TOKEN = "ghp_..." before running this script.
# Default: $null (anonymous, lower rate limit but still works for public repo logs).
$token = $env:GITHUB_TOKEN
$h = @{"User-Agent"="PowerShell"}
if ($token) { $h["Authorization"] = "token $token" }

if ($PrNumber -eq 0) {
    Write-Host "Usage: pwsh gh_pr_detail.ps1 <pr_number>"
    exit
}

try {
    $pr = Invoke-RestMethod -Uri "https://api.github.com/repos/YintaTriss/apeireth-rust/pulls/$PrNumber" -Method Get -TimeoutSec 30 -Headers $h -SkipCertificateCheck
    Write-Host "=== PR #$($pr.number): $($pr.title) ==="
    Write-Host "  state: $($pr.state) | draft: $($pr.draft)"
    Write-Host "  author: $($pr.user.login)"
    Write-Host "  base: $($pr.base.ref) (sha=$($pr.base.sha.Substring(0,7)))"
    Write-Host "  head: $($pr.head.ref) (sha=$($pr.head.sha.Substring(0,7)))"
    Write-Host "  url: $($pr.html_url)"
    Write-Host "  created: $($pr.created_at) | updated: $($pr.updated_at)"
    Write-Host "  +$($pr.additions) / -$($pr.deletions) | $($pr.changed_files) files"
    Write-Host "  mergeable: $($pr.mergeable) | mergeable_state: $($pr.mergeable_state)"
    Write-Host "  base ref oid: $($pr.base.repo.id)"
    Write-Host ""
    Write-Host "=== Body ==="
    Write-Host $pr.body
    Write-Host ""
    Write-Host "=== CI status ==="
    $ci = Invoke-RestMethod -Uri "https://api.github.com/repos/YintaTriss/apeireth-rust/commits/$($pr.head.sha)/check-runs?per_page=20" -Method Get -TimeoutSec 30 -Headers $h -SkipCertificateCheck
    if ($ci.check_runs.Count -eq 0) {
        Write-Host "  (no check runs yet)"
    } else {
        $ci.check_runs | ForEach-Object {
            Write-Host "  $($_.name) - $($_.conclusion) ($($_.status))"
        }
    }
    Write-Host ""
    Write-Host "=== Files changed (top 20) ==="
    $files = Invoke-RestMethod -Uri "https://api.github.com/repos/YintaTriss/apeireth-rust/pulls/$PrNumber/files?per_page=100" -Method Get -TimeoutSec 30 -Headers $h -SkipCertificateCheck
    $files | Select-Object -First 20 | ForEach-Object {
        Write-Host "  $($_.status) +$($_.additions) -$($_.deletions) $($_.filename)"
    }
} catch {
    Write-Host "Err: $_"
}