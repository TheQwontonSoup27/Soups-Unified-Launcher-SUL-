# Turns the cause of a failed cargo step into one annotation on the run.
#
# The raw job logs cannot be downloaded without signing in to GitHub, and the run page only says
# "Process completed with exit code 1". Annotations are readable through the API, so the lines that
# say what failed are put there. Only the failing step calls this; a green run prints nothing.
param([Parameter(Mandatory)][string[]]$Logs)

$all = @(foreach ($log in $Logs) { if (Test-Path $log) { Get-Content $log | ForEach-Object { "$_" } } })

# Compiler and clippy errors, test panics and the list of failed tests. "Compiling thiserror" lines
# are not matched: the pattern needs `error` at the start of a line or as `error:` after a location.
$pattern = '^error(\[E\d+\])?:|\.rs:\d+:\d+: (error|warning)|panicked at|^---- |FAILED|^failures:'
$hits = @($all | Where-Object { $_ -match $pattern } | Select-Object -First 25)
$tail = @($all | Select-Object -Last 40)

$text = (@('First matches:') + $hits + @('', 'Last lines:') + $tail) -join "`n"
$text = $text -replace '%', '%25' -replace "`r", '%0D' -replace "`n", '%0A'
Write-Host "::error title=Cargo step failed::$text"
