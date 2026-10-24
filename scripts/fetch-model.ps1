# Downloads the GLiNER model used by redactor's optional "Deep scan".
# Windows counterpart of fetch-model.sh: same files, revision and hashes.
#
# The app never downloads anything by itself: run this once, by hand. The
# files come from a pinned Hugging Face revision and are checked against
# the SHA-256 hashes below before they are moved into place.
#
# Model: knowledgator/gliner-pii-edge-v1.0 (Apache-2.0), quantized ONNX.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\fetch-model.ps1 [destination dir]
#        default: $env:REDACTOR_CONFIG_DIR, $env:XDG_CONFIG_HOME\redactor or %USERPROFILE%\.config\redactor

param([string]$Destination)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue' # Invoke-WebRequest is much faster without it
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = 'knowledgator/gliner-pii-edge-v1.0'
$Revision = '9b7f39b0a2da971a5beea78d35f1539d4009c891'
$Name = 'gliner-pii-edge-v1.0'

# Same lookup as redactor_core::store::default_dir.
if ($env:REDACTOR_CONFIG_DIR) {
    $Config = $env:REDACTOR_CONFIG_DIR
} elseif ($env:XDG_CONFIG_HOME) {
    $Config = Join-Path $env:XDG_CONFIG_HOME 'redactor'
} else {
    $Config = Join-Path (Join-Path $env:USERPROFILE '.config') 'redactor'
}
if (-not $Destination) {
    $Destination = Join-Path (Join-Path $Config 'models') $Name
}

# path in the repo, local name, sha256
$Files = @(
    @('onnx/model_quint8.onnx', 'model.onnx', '988acb03456b26e2d9f2521016d820310c2ed64deb4a846297d3289f0c2eb7e4'),
    @('tokenizer.json', 'tokenizer.json', '84b3a9b18f04a0ccd03b72d9f871b7e0bec40fd7021ef50bc30a7c3693c11205'),
    @('gliner_config.json', 'gliner_config.json', '77e6b57335c4bfd461e9041682196dd6c373a0b09bbd9269ef9e95b807915340')
)

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("redactor-model-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    foreach ($f in $Files) {
        $remote, $local, $expected = $f
        $url = "https://huggingface.co/$Repo/resolve/$Revision/$remote"
        Write-Host "downloading $remote"
        Invoke-WebRequest -Uri $url -OutFile (Join-Path $Tmp $local) -UseBasicParsing
        $actual = (Get-FileHash -Algorithm SHA256 (Join-Path $Tmp $local)).Hash.ToLower()
        if ($actual -ne $expected) {
            Write-Error "checksum mismatch for $remote`n  expected $expected`n  actual   $actual"
        }
    }
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Move-Item -Force -Path (Join-Path $Tmp '*') -Destination $Destination
    Write-Host "model ready in $Destination"
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
