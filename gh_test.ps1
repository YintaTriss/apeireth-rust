$url = "https://api.github.com/repos/YintaTriss/apeireth-rust/actions/runs?per_page=5"
$h = @{"User-Agent"="PowerShell"}
try {
    $r = Invoke-RestMethod -Uri $url -Method Get -TimeoutSec 30 -Headers $h -SkipCertificateCheck
    Write-Host "OK total=$($r.total_count)"
    $r.workflow_runs | Select-Object name, head_sha, conclusion, created_at | Format-Table -AutoSize | Out-String -Width 200
} catch {
    Write-Host "Err: $_"
}