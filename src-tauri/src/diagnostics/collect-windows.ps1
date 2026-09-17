param([int]$WindowDays = 60)
# TaskForge stability collector (Windows).
# Read-only: queries event logs, CIM/WMI and the registry, changes nothing.
# Emits exactly one JSON document on stdout; the analysis lives in the frontend
# (src/lib/diagnostics.ts). Keep this file ASCII-only: it is fed over stdin.

$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false

$now = Get-Date
$since = $now.AddDays(-$WindowDays)
# WER kernel reports are deduplicated below, so a longer look-back stays cheap and
# shows when a recurring GPU/driver fault first appeared.
$reportSince = $now.AddDays(-[Math]::Max($WindowDays, 180))
$notes = @()

function Ts($d) {
  if ($d) { ([DateTimeOffset]$d).ToUnixTimeSeconds() } else { $null }
}

function Clip([string]$s, [int]$n = 400) {
  if (-not $s) { return '' }
  $s = ($s -replace '\s+', ' ').Trim()
  if ($s.Length -gt $n) { $s.Substring(0, $n) + '...' } else { $s }
}

# Some providers (nvlddmkm) ship no message template; fall back to the raw insertion strings.
function EventText($e) {
  $m = $e.Message
  if (-not $m) { $m = (@($e.Properties | Select-Object -First 2 | ForEach-Object { "$($_.Value)" }) -join ' ') }
  Clip $m
}

function EventData($e) {
  $h = @{}
  try {
    ([xml]$e.ToXml()).Event.EventData.Data | ForEach-Object { if ($_.Name) { $h[$_.Name] = $_.'#text' } }
  } catch {}
  $h
}

$elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
  [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $elevated) {
  $notes += 'Not running as administrator: disk reliability counters, live kernel dumps and some crash dump files may be unreadable.'
}

# ---- System ---------------------------------------------------------------
$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
$bios = Get-CimInstance Win32_BIOS
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1

$microcode = $null
try {
  $b = [byte[]](Get-ItemProperty 'HKLM:\HARDWARE\DESCRIPTION\System\CentralProcessor\0').'Update Revision'
  if ($b.Length -ge 8) { $microcode = '0x{0:X}' -f [BitConverter]::ToUInt32($b, 4) }
  elseif ($b.Length -ge 4) { $microcode = '0x{0:X}' -f [BitConverter]::ToUInt32($b, 0) }
} catch {}

$system = [pscustomobject]@{
  os          = "$($os.Caption) (build $($os.BuildNumber))"
  model       = "$($cs.Manufacturer) $($cs.Model)".Trim()
  bios        = "$($bios.SMBIOSBIOSVersion)"
  biosDate    = Ts $bios.ReleaseDate
  biosAgeDays = $(if ($bios.ReleaseDate) { [int]($now - $bios.ReleaseDate).TotalDays } else { $null })
  cpu         = "$($cpu.Name)".Trim()
  microcode   = $microcode
  ramGb       = [math]::Round($cs.TotalPhysicalMemory / 1GB, 1)
  lastBoot    = Ts $os.LastBootUpTime
}

# ---- Drivers that most often destabilize a system ------------------------
$drivers = @(Get-CimInstance Win32_PnPSignedDriver |
  Where-Object {
    $_.DeviceName -and
    ($_.DeviceClass -in 'DISPLAY', 'NET' -or $_.DeviceName -match 'Management Engine|Chipset|NVM Express|Dynamic Tuning|Serial IO|Rapid Storage') -and
    $_.DeviceName -notmatch 'WAN Miniport|Kernel Debug|Virtual|Hyper-V|Bluetooth Device'
  } |
  Sort-Object DeviceName -Unique |
  ForEach-Object {
    [pscustomobject]@{
      device   = $_.DeviceName
      class    = "$($_.DeviceClass)"
      provider = "$($_.DriverProviderName)"
      version  = $_.DriverVersion
      date     = Ts $_.DriverDate
      ageDays  = $(if ($_.DriverDate) { [int]($now - $_.DriverDate).TotalDays } else { $null })
    }
  })

# ---- Unexpected shutdowns (Kernel-Power 41) ------------------------------
# 6008 carries the last time the system was known alive, as localized text.
$dirty = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'EventLog'; Id = 6008; StartTime = $since } |
  ForEach-Object {
    $alive = $null
    try {
      $txt = ("$($_.Properties[1].Value) $($_.Properties[0].Value)" -replace '[^\x20-\x7E]', '')
      $alive = Ts ([datetime]::Parse($txt))
    } catch {}
    [pscustomobject]@{ ts = Ts $_.TimeCreated; lastAlive = $alive }
  })

$shutdowns = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-Power'; Id = 41; StartTime = $since } |
  ForEach-Object {
    $d = EventData $_
    $ts = Ts $_.TimeCreated
    $pair = $dirty | Where-Object { [math]::Abs($_.ts - $ts) -lt 900 } | Select-Object -First 1
    $code = 0
    [int64]::TryParse("$($d.BugcheckCode)", [ref]$code) | Out-Null
    [pscustomobject]@{
      ts               = $ts
      lastAlive        = $(if ($pair) { $pair.lastAlive } else { $null })
      bugcheck         = $code
      params           = (@($d.BugcheckParameter1, $d.BugcheckParameter2, $d.BugcheckParameter3, $d.BugcheckParameter4) -join ', ')
      sleepInProgress  = ("$($d.SleepInProgress)" -notin '', '0')
      resumedFromSleep = ([int]"0$($d.SystemSleepTransitionsToOn)" -gt 0)
      powerButton      = ("$($d.PowerButtonTimestamp)" -notin '', '0')
      longPowerPress   = ("$($d.LongPowerButtonPressDetected)" -eq 'true')
    }
  })

# ---- Restarts requested by software (Windows Update and others) ---------
$restarts = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'User32'; Id = 1074; StartTime = $since } |
  ForEach-Object {
    [pscustomobject]@{
      ts      = Ts $_.TimeCreated
      process = "$($_.Properties[0].Value)"
      reason  = "$($_.Properties[2].Value)"
      action  = "$($_.Properties[4].Value)"
      user    = "$($_.Properties[6].Value)"
    }
  })

# ---- Blue screens (WER-SystemErrorReporting 1001) ------------------------
$bugchecks = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WER-SystemErrorReporting'; Id = 1001; StartTime = $since } |
  ForEach-Object {
    $code = $null; $params = $null
    if ("$($_.Properties[0].Value)" -match '(0x[0-9a-fA-F]+)\s*\(([^)]*)\)') { $code = $matches[1]; $params = $matches[2] }
    [pscustomobject]@{ ts = Ts $_.TimeCreated; code = $code; params = $params; dump = "$($_.Properties[1].Value)" }
  })

# ---- Live kernel events / blue screen reports (deduplicated) -------------
# WER re-queues the same report many times, so group by kind + code + first parameter.
$kernelReports = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Windows Error Reporting'; Id = 1001; StartTime = $reportSince } |
  Where-Object { "$($_.Properties[2].Value)" -match '^(LiveKernelEvent|BlueScreen)$' } |
  Group-Object { "$($_.Properties[2].Value)|$($_.Properties[5].Value)|$($_.Properties[6].Value)" } |
  ForEach-Object {
    $t = @($_.Group | Sort-Object TimeCreated)
    $k = $_.Name -split '\|'
    [pscustomobject]@{ kind = $k[0]; code = $k[1]; param1 = $k[2]; submissions = $_.Count; first = Ts $t[0].TimeCreated; last = Ts $t[-1].TimeCreated }
  })

# ---- Categorized System log warnings/errors ------------------------------
$categories = @(
  @{ cat = 'gpu';      re = '^(nvlddmkm|amdkmdag|amdwddmg|igfx\w*|Display|Microsoft-Windows-DxgKrnl)$' }
  @{ cat = 'hardware'; re = '^Microsoft-Windows-WHEA-Logger$' }
  @{ cat = 'thermal';  re = '^Microsoft-Windows-Kernel-Processor-Power$'; ids = @(37) }
  @{ cat = 'storage';  re = '^(disk|Ntfs|Microsoft-Windows-Ntfs|stornvme|storahci|iaStor\w*|volmgr|volsnap|Microsoft-Windows-StorPort)$' }
  @{ cat = 'network';  re = '^(Netw[a-z]{2}\d*|e1[a-z]express|e2f\w*|rt640x64|rtwlan\w*|mtkwl\w*|Qcamain\w*|Microsoft-Windows-NDIS)$' }
  @{ cat = 'memory';   re = '^Microsoft-Windows-Resource-Exhaustion-Detector$' }
  @{ cat = 'update';   re = '^Microsoft-Windows-WindowsUpdateClient$' }
  @{ cat = 'service';  re = '^Service Control Manager$'; ids = @(7031, 7034) }
)
$events = New-Object System.Collections.ArrayList
$perCat = @{}
foreach ($e in (Get-WinEvent -FilterHashtable @{ LogName = 'System'; Level = 1, 2, 3; StartTime = $since })) {
  foreach ($c in $categories) {
    if ($e.ProviderName -match $c.re -and (-not $c.ids -or $c.ids -contains $e.Id)) {
      if ([int]$perCat[$c.cat] -lt 250) {
        $perCat[$c.cat] = [int]$perCat[$c.cat] + 1
        [void]$events.Add([pscustomobject]@{
            ts = Ts $e.TimeCreated; source = $e.ProviderName; id = $e.Id; level = [int]$e.Level
            category = $c.cat; message = EventText $e
          })
      }
      break
    }
  }
}

# ---- Application crashes and hangs ---------------------------------------
$appFaults = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; Id = 1000, 1002; StartTime = $since } |
  Where-Object { $_.ProviderName -in 'Application Error', 'Application Hang' } |
  ForEach-Object {
    [pscustomobject]@{ app = "$($_.Properties[0].Value)"; kind = $(if ($_.Id -eq 1002) { 'hang' } else { 'crash' }); ts = Ts $_.TimeCreated }
  } |
  Group-Object app, kind |
  ForEach-Object {
    $t = @($_.Group | Sort-Object ts)
    [pscustomobject]@{
      app = $t[0].app; kind = $t[0].kind; count = $_.Count; first = $t[0].ts; last = $t[-1].ts
      times = @($t | Select-Object -Last 25 | ForEach-Object { $_.ts })
    }
  })

# ---- Storage health -------------------------------------------------------
$disks = @(Get-PhysicalDisk | ForEach-Object {
    # Needs admin; unelevated it just yields nulls.
    $rc = $_ | Get-StorageReliabilityCounter 2>$null
    [pscustomobject]@{
      name = $_.FriendlyName; media = "$($_.MediaType)"; bus = "$($_.BusType)"
      health = "$($_.HealthStatus)"; operational = "$($_.OperationalStatus)"; sizeGb = [math]::Round($_.Size / 1GB)
      wearPct = $rc.Wear; tempC = $rc.Temperature; tempMaxC = $rc.TemperatureMax
      readErrors = $rc.ReadErrorsUncorrected; writeErrors = $rc.WriteErrorsUncorrected; powerOnHours = $rc.PowerOnHours
    }
  })
$volumes = @(Get-Volume | Where-Object DriveLetter | ForEach-Object {
    [pscustomobject]@{
      drive = "$($_.DriveLetter):"; fs = $_.FileSystem; health = "$($_.HealthStatus)"
      sizeGb = [math]::Round($_.Size / 1GB); freeGb = [math]::Round($_.SizeRemaining / 1GB)
    }
  })

# ---- Configuration that affects crash capture and stability --------------
$cc = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\CrashControl'
$minidumps = $null
try { $minidumps = @([IO.Directory]::GetFiles("$env:SystemRoot\Minidump", '*.dmp')).Count } catch {}
$liveDumps = $null
try {
  $liveDumps = @([IO.Directory]::GetFiles("$env:SystemRoot\LiveKernelReports", '*.dmp', 'AllDirectories') |
    ForEach-Object { (Split-Path (Split-Path $_ -Parent) -Leaf) + '\' + (Split-Path $_ -Leaf) })
} catch {}
$dg = Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard -ClassName Win32_DeviceGuard
$memDiag = Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-MemoryDiagnostics-Results' } -MaxEvents 1
$bootEvents = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-General'; Id = 12, 13; StartTime = $since })
$powerPlan = "$(powercfg /getactivescheme)" -replace '^.*\((.*)\)\s*$', '$1'
$hiberboot = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power').HiberbootEnabled

$config = [pscustomobject]@{
  crashDumpMode     = $cc.CrashDumpEnabled
  autoReboot        = [bool]$cc.AutoReboot
  minidumpCount     = $minidumps
  memoryDumpPresent = [bool](Test-Path "$env:SystemRoot\MEMORY.DMP")
  liveKernelDumps   = $liveDumps
  fastStartup       = $(if ($null -ne $hiberboot) { $hiberboot -eq 1 } else { $null })
  hypervisorPresent = [bool]$cs.HypervisorPresent
  vbsRunning        = $(if ($dg) { $dg.VirtualizationBasedSecurityStatus -eq 2 } else { $null })
  hvciRunning       = $(if ($dg) { @($dg.SecurityServicesRunning) -contains 2 } else { $null })
  pagefileAuto      = [bool]$cs.AutomaticManagedPagefile
  commitUsedGb      = [math]::Round(($os.TotalVirtualMemorySize - $os.FreeVirtualMemory) / 1MB, 1)
  commitLimitGb     = [math]::Round($os.TotalVirtualMemorySize / 1MB, 1)
  memoryDiagnostic  = $(if ($memDiag) { [pscustomobject]@{ ts = Ts $memDiag.TimeCreated; text = EventText $memDiag } } else { $null })
  powerPlan         = $powerPlan
  bootsInWindow     = @($bootEvents | Where-Object Id -eq 12).Count
  cleanShutdowns    = @($bootEvents | Where-Object Id -eq 13).Count
  topMemory         = @(Get-Process | Sort-Object PrivateMemorySize64 -Descending | Select-Object -First 6 | ForEach-Object {
      [pscustomobject]@{ name = $_.Name; pid = $_.Id; privateGb = [math]::Round($_.PrivateMemorySize64 / 1GB, 2) }
    })
}

$report = [pscustomobject]@{
  schema        = 1
  platform      = 'windows'
  collectedAt   = Ts $now
  windowDays    = $WindowDays
  elevated      = $elevated
  system        = $system
  shutdowns     = $shutdowns
  restarts      = $restarts
  bugchecks     = $bugchecks
  kernelReports = $kernelReports
  events        = @($events)
  appFaults     = $appFaults
  disks         = $disks
  volumes       = $volumes
  drivers       = $drivers
  config        = $config
  notes         = @($notes)
}
ConvertTo-Json -InputObject $report -Depth 6 -Compress
