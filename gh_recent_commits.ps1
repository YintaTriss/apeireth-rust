param([int]$Count = 15)
# Privacy: 0 hardcode GitHub PAT in scripts. Read from $env:GITHUB_TOKEN
# Set $env:GITHUB_TOKEN = "ghp_..." before running this script.
# Default: $null (anonymous, lower rate limit but still works for public repo logs).
$token = $env:GITHUB_TOKEN
$h = @{"User-Agent"="PowerShell"}
if ($token) { $h["Authorization"] = "token $token" }

try {
    $commits = Invoke-RestMethod -Uri "https://api.github.com/repos/YintaTriss/apeireth-rust/commits?sha=master&per_page=$Count" -Method Get -TimeoutSec 30 -Headers $h -SkipCertificateCheck
    Write-Host "=== Master last $Count commits ==="
    $commits | ForEach-Object {
        $msg = $_.commit.message -split "`n" | Select-Object -First 1
        Write-Host "  $($_.sha.Substring(0,7)) $msg [$($_.commit.author.name)]"
    }
} catch {
    Write-Host "Err: $_"
}