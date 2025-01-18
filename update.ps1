$command="mkv-subtitle-extractor.exe"
$old_exe = Get-Command $command | Select-Object -ExpandProperty Source
if (-not (Test-Path $old_exe)) {
    Write-Error "Could not find $command in your path!"
    return
}
$new_exe = "target\release\$command"
if (-not (Test-Path $new_exe)) {
    Write-Error "Could not find target exe, run `cargo build --release` please."
    return
}
Copy-Item -Path $new_exe -Destination $old_exe
Write-Host "Now in path:"
Invoke-Expression "$command --version"