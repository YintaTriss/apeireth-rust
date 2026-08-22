param([long]$RunId = 0)
# Privacy: 0 hardcode GitHub PAT in scripts. Read from $env:GITHUB_TOKEN
# Set $env:GITHUB_TOKEN = "ghp_..." before running this script.
# Default: $null (anonymous, lower rate limit but still works for public repo logs).
$token = $env:GITHUB_TOKEN
$h = @{"User-Agent"="PowerShell"}
if ($token) { $h["Authorization"] = "token $token" }

if ($RunId -eq 0) {
    Write-Host "Usage: pwsh gh_get_one_log.ps1 <run_id>"
    exit
}

try {
    $r = Invoke-WebRequest -Uri "https://api.github.com/repos/YintaTriss/apeireth-rust/actions/runs/$RunId/logs" -Method Get -MaximumRedirection 10 -Headers $h -OutFile "$env:TEMP\log_one.zip"
    $size = (Get-Item "$env:TEMP\log_one.zip").Length
    Write-Host "Downloaded $size bytes"
    Expand-Archive -Path "$env:TEMP\log_one.zip" -DestinationPath "$env:TEMP\log_one_extract" -Force
    Get-ChildItem "$env:TEMP\log_one_extract" -Recurse | Select-Object FullName
} catch {
    Write-Host "Err: $_"
}