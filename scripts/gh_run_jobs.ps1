param([long]$RunId = 0)
# Privacy: 0 hardcode GitHub PAT in scripts. Read from $env:GITHUB_TOKEN
$token = $env:GITHUB_TOKEN
$h = @{"User-Agent"="PowerShell";"Authorization"="token $token"}

if ($RunId -eq 0) {
    Write-Host "Usage: pwsh scripts/gh_run_jobs.ps1 <run_id>"
    exit
}

try {
    $jobs = Invoke-RestMethod -Uri "https://api.github.com/repos/YintaTriss/apeireth-rust/actions/runs/$RunId/jobs" -Method Get -TimeoutSec 30 -Headers $h -SkipCertificateCheck
    Write-Host "=== Run $RunId ==="
    $jobs.jobs | ForEach-Object {
        Write-Host ""
        Write-Host "Job: $($_.name)"
        Write-Host "  conclusion: $($_.conclusion)"
        $_.steps | ForEach-Object {
            $marker = if ($_.conclusion -eq "failure") { "FAIL" } elseif ($_.conclusion -eq "success") { "OK" } else { "..." }
            Write-Host "  [$marker] Step $($_.number): $($_.name) ($($_.conclusion))"
        }
    }
} catch {
    Write-Host "Err: $_"
}