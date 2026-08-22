param([string]$Sha, [int]$WaitSec = 60)
if (-not $Sha) {
    $Sha = git rev-parse HEAD 2>$null
}
# Privacy: 0 hardcode GitHub PAT in scripts. Read from $env:GITHUB_TOKEN
$token = $env:GITHUB_TOKEN
$h = @{"User-Agent"="PowerShell";"Authorization"="token $token"}

Write-Host "Waiting $WaitSec seconds for CI to pick up commit $Sha..."
Start-Sleep -Seconds $WaitSec

try {
    $runs = Invoke-RestMethod -Uri "https://api.github.com/repos/YintaTriss/apeireth-rust/actions/runs?sha=$Sha" -Method Get -TimeoutSec 30 -Headers $h -SkipCertificateCheck
    Write-Host "=== Runs for $($Sha.Substring(0,7)) ==="
    $runs.workflow_runs | Group-Object conclusion | Select-Object Count, Name | Format-Table -AutoSize
    Write-Host ""
    $runs.workflow_runs | ForEach-Object {
        $marker = if ($_.conclusion -eq "failure") { "FAIL" } elseif ($_.conclusion -eq "success") { "OK" } elseif ($_.conclusion -eq $null) { "RUN" } else { "? $($_.conclusion)" }
        Write-Host "  [$marker] $($_.name) (id=$($_.id))"
    }
} catch {
    Write-Host "Err: $_"
}