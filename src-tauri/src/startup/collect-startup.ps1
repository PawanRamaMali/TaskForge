param([switch]$Apply, [string]$ChangesFile, [string]$OutFile)
# Startup manager collector/applier (Windows).
#   default: enumerate startup entries and emit one JSON document on stdout.
#   -Apply -ChangesFile <path>: apply enable/disable changes from that JSON file,
#     needs admin for machine-scope entries. Read-only otherwise.
# ASCII-only: this file is fed to PowerShell over stdin.

$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false

# StartupApproved binary: byte 0 bit 0 set => disabled. 02.. = enabled, 03.. = disabled.
$ENABLED_BIN  = [byte[]](2,0,0,0,0,0,0,0,0,0,0,0)
$DISABLED_BIN = [byte[]](3,0,0,0,0,0,0,0,0,0,0,0)

# Run keys and the StartupApproved key that holds their enabled/disabled flag.
$RUN_SPECS = @(
  @{ kind='ru';  run='HKCU:\Software\Microsoft\Windows\CurrentVersion\Run';                approved='HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run';    scope='user' }
  @{ kind='rm';  run='HKLM:\Software\Microsoft\Windows\CurrentVersion\Run';                approved='HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run';    scope='machine' }
  @{ kind='r32'; run='HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run';    approved='HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32';  scope='machine' }
)
$FOLDER_SPECS = @(
  @{ kind='sfu'; dir=[Environment]::GetFolderPath('Startup');       approved='HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder'; scope='user' }
  @{ kind='sfm'; dir=[Environment]::GetFolderPath('CommonStartup'); approved='HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder'; scope='machine' }
)

function Approved-Enabled($approvedKey, $name) {
  $v = (Get-ItemProperty -Path $approvedKey -Name $name -ErrorAction SilentlyContinue).$name
  if (-not $v) { return $true }           # no flag recorded => enabled
  return ($v[0] -band 1) -eq 0
}

# Windows itself puts no boot-critical items in these locations, so "essential"
# here just means Microsoft-signed, shown so the user can keep those if they want.
function Is-Microsoft($path) {
  if (-not $path) { return $false }
  $exe = $path.Trim('"'); if ($exe -match '^(.*?\.exe)') { $exe = $matches[1] }
  if (-not (Test-Path $exe)) { return ($path -match '(?i)\\Windows\\') }
  $sig = Get-AuthenticodeSignature $exe -ErrorAction SilentlyContinue
  return ($sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -match 'O=Microsoft Corporation')
}

function New-Id($kind, $name) { "$kind|$name" }

$items = New-Object System.Collections.ArrayList

foreach ($s in $RUN_SPECS) {
  $props = (Get-Item -Path $s.run -ErrorAction SilentlyContinue).Property
  foreach ($name in $props) {
    $cmd = (Get-ItemProperty -Path $s.run -Name $name).$name
    [void]$items.Add([pscustomobject]@{
      id = New-Id $s.kind $name; name = $name; command = "$cmd"
      source = "Run ($($s.scope))"; scope = $s.scope; kind = $s.kind
      microsoft = [bool](Is-Microsoft "$cmd"); enabled = [bool](Approved-Enabled $s.approved $name)
    })
  }
}

foreach ($s in $FOLDER_SPECS) {
  Get-ChildItem -Path $s.dir -Filter *.lnk -ErrorAction SilentlyContinue | ForEach-Object {
    $target = (New-Object -ComObject WScript.Shell).CreateShortcut($_.FullName).TargetPath
    [void]$items.Add([pscustomobject]@{
      id = New-Id $s.kind $_.Name; name = $_.BaseName; command = "$target"
      source = "Startup folder ($($s.scope))"; scope = $s.scope; kind = $s.kind
      microsoft = [bool](Is-Microsoft "$target"); enabled = [bool](Approved-Enabled $s.approved $_.Name)
    })
  }
}

# Scheduled tasks that trigger at logon/boot, excluding the Microsoft\ tree (OS tasks).
foreach ($t in (Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object {
      $_.TaskPath -notlike '\Microsoft\*' -and
      ($_.Triggers.CimClass.CimClassName -contains 'MSFT_TaskLogonTrigger' -or
       $_.Triggers.CimClass.CimClassName -contains 'MSFT_TaskBootTrigger')
    })) {
  $full = ($t.TaskPath + $t.TaskName)
  $action = ($t.Actions | Where-Object { $_.Execute } | Select-Object -First 1).Execute
  [void]$items.Add([pscustomobject]@{
    id = New-Id 'st' $full; name = $t.TaskName; command = "$action"
    source = 'Scheduled task'; scope = 'machine'; kind = 'st'
    microsoft = [bool](Is-Microsoft "$action"); enabled = ($t.State -ne 'Disabled')
  })
}

if (-not $Apply) {
  ConvertTo-Json -InputObject @{ items = @($items) } -Depth 5 -Compress
  return
}

# ---- Apply mode -----------------------------------------------------------
$changes = Get-Content -Raw -Path $ChangesFile | ConvertFrom-Json
$results = New-Object System.Collections.ArrayList
foreach ($c in $changes) {
  $item = $items | Where-Object { $_.id -eq $c.id } | Select-Object -First 1
  $ok = $false; $err = $null
  try {
    if (-not $item) { throw 'not found' }
    $enable = [bool]$c.enabled
    if ($item.kind -eq 'st') {
      $p, $n = ($item.id.Substring(3) -replace '\\([^\\]*)$', "`0`$1") -split "`0", 2
      if ($enable) { Enable-ScheduledTask -TaskPath $p -TaskName $n -ErrorAction Stop | Out-Null }
      else { Disable-ScheduledTask -TaskPath $p -TaskName $n -ErrorAction Stop | Out-Null }
    } else {
      $spec = ($RUN_SPECS + $FOLDER_SPECS) | Where-Object { $_.kind -eq $item.kind } | Select-Object -First 1
      if (-not (Test-Path $spec.approved)) { New-Item -Path $spec.approved -Force | Out-Null }
      $valueName = $item.id.Substring($item.kind.Length + 1)
      $bin = if ($enable) { $ENABLED_BIN } else { $DISABLED_BIN }
      New-ItemProperty -Path $spec.approved -Name $valueName -PropertyType Binary -Value $bin -Force | Out-Null
    }
    $ok = $true
  } catch { $err = "$($_.Exception.Message)" }
  [void]$results.Add(@{ id = $c.id; ok = $ok; error = $err })
}
$json = ConvertTo-Json -InputObject @{ results = @($results) } -Depth 4 -Compress
# Elevated runs can't have their stdout redirected by the caller, so also write a file.
if ($OutFile) { Set-Content -Path $OutFile -Value $json -Encoding UTF8 }
$json
