# Benchmark-Suite fuer spoor (siehe docs/prompts.md, Abschnitt Benchmark).
# Laeuft komplett offline und mit festen Seeds -> deterministisch fuer einen
# gegebenen Datenbestand. Ausgabe: benchmarks/latest.txt + run-<datum>.txt.

$ErrorActionPreference = "Stop"
# spoor schreibt UTF-8; ohne das liest PowerShell die Ausgabe je nach
# Konsolen-Codepage falsch und die Ergebnisdatei enthaelt Mojibake.
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

cargo build --release 2>&1 | Out-Null
$spoor = Join-Path $repo "target\release\spoor.exe"

New-Item -ItemType Directory -Force (Join-Path $repo "benchmarks") | Out-Null
$stamp = Get-Date -Format "yyyy-MM-dd_HHmm"
$outfile = Join-Path $repo "benchmarks\latest.txt"

$findQueries = [ordered]@{
    "B01" = "Baum"
    "B02" = "Werkzeug für Wald und Baum"
    "B03" = "CLI die Logs von verteilten Systemen synchronisiert"
    "B04" = "Wasser Licht"
    "B05" = "sky thunder king"
    "B06" = "track meaning to its origin"
    "B07" = "audio dispatch senden kommunikation genuss spiel musik sonne"
    "B08" = "schwarz logs lesen forensisch skalpell schneiden lupe suchen präzise"
    "B09" = "weisheit erkenntnis lehre"
    "B10" = "feuer schmiede handwerk"
    "B11" = "track hunt trace search"
    "B12" = "himmel donner goetter blitz"
}

$lines = @()
$lines += "spoor benchmark  ($stamp)"
$lines += "----------------------------------------"
# Bestandsgroesse in den Kopf, damit Diffs einordbar sind
$lines += (& $spoor db info 2>$null | Select-Object -First 1)
$lines += ""

foreach ($key in $findQueries.Keys) {
    $q = $findQueries[$key]
    $lines += "## $key  find `"$q`""
    # stderr (Quelle:-Zeile) verwerfen, stdout ist das Messobjekt
    $result = & $spoor find $q --offline --count 3 --explain 2>$null
    if ($LASTEXITCODE -ne 0) { $result = @("(keine Treffer)") }
    $lines += $result
    $lines += ""
}

$lines += "## G01  gen --seed 42 --count 5"
$lines += (& $spoor gen --seed 42 --count 5 2>$null)
$lines += ""
$lines += "## G02  gen --seed 42 --count 3 --systems nature"
$lines += (& $spoor gen --seed 42 --count 3 --systems nature 2>$null)
$lines += ""

$lines | Set-Content -Encoding utf8 $outfile
Copy-Item $outfile (Join-Path $repo "benchmarks\run-$stamp.txt")

Write-Host "Benchmark geschrieben: benchmarks\latest.txt (Kopie: run-$stamp.txt)"
Write-Host "Diff zum vorherigen Lauf:  git diff --no-index benchmarks\run-<alt>.txt benchmarks\latest.txt"
