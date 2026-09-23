# Builds and stages the renderer as a Windows x64 native shared library.
#
# UNTESTED: this script was authored on macOS and has never executed in this repository.
# Verify on Windows from the repository root with:
#   powershell -ExecutionPolicy Bypass -File .\dioxus-compose-renderer\desktop\scripts\build-native-windows.ps1
#   powershell -ExecutionPolicy Bypass -File .\dioxus-compose-renderer\desktop\scripts\smoke-test-windows.ps1 -RequireClick
#
# Distribution layout:
#   build\native-image\dist\
#     bin\libdioxus_compose_renderer.dll  renderer, with the JDK's desktop libraries in it
#     bin\skiko-windows-x64.dll           Skia loaded by Skiko by path
#     bin\icudtl.dat                      Skia ICU data
#     lib\fontconfig.bfc                  AWT font configuration
#
# Windows builds with Liberica NIK, the same toolchain as macOS. Upstream GraalVM also
# supports AWT here, and that is what this used to use, but it supports it by emitting a
# dozen JDK DLLs beside the image for every application to carry. NIK ships those same
# libraries as static archives, so they link in and stop being files. It still does not
# need a placeholder toolkit DLL, a JAWT forwarder, JNI_OnLoad_osxui or an AppKit
# main-thread pump: AWT creates and owns the Win32 event-dispatch thread.
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Runs a native command without letting its progress output count as a failure.
#
# With $ErrorActionPreference set to Stop, PowerShell turns anything a native command
# writes to stderr into a terminating error, even when the command exits 0. The Kotlin CLI
# reports its downloads there, and native-image reports its progress there, so both looked
# like failures on a machine that had never run them before. Exit codes are the only
# signal worth trusting here, and every caller already checks $LASTEXITCODE.
function Invoke-Native {
    param([scriptblock]$Command)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $Command } finally { $ErrorActionPreference = $previous }
}

function Fail([string]$Message, [string[]]$Hints = @()) {
    [Console]::Error.WriteLine("error: $Message")
    foreach ($hint in $Hints) {
        [Console]::Error.WriteLine("       $hint")
    }
    exit 1
}

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    Fail "build-native-windows.ps1 must run on Windows x64" @(
        "native-image does not cross-compile; use a Windows x64 machine or runner."
    )
}
if (-not [Environment]::Is64BitOperatingSystem -or -not [Environment]::Is64BitProcess) {
    Fail "Windows x64 and a 64-bit PowerShell process are required" @(
        "Open x64 PowerShell, not the x86 shell."
    )
}

$ScriptsDir = $PSScriptRoot
$NativeDir = Split-Path -Parent $ScriptsDir
$ProjectDir = Split-Path -Parent $NativeDir
$BuildDir = Join-Path $ProjectDir "build\native-image"
$DistDir = Join-Path $BuildDir "dist"
$BinDir = Join-Path $DistDir "bin"
$LibDir = Join-Path $DistDir "lib"
$ObjDir = Join-Path $BuildDir "obj-windows"
$MetadataDir = Join-Path $ScriptsDir "windows-metadata"
$LibraryName = "libdioxus_compose_renderer"
$RendererSource = Join-Path $NativeDir "c\renderer_entry.c"
$KotlinWrapper = Join-Path $ProjectDir "kotlin.bat"
$ClasspathFile = Join-Path $BuildDir "classpath-windows.txt"
$JvmLog = Join-Path $BuildDir "jvm-run-windows.log"

if (-not (Test-Path -LiteralPath $RendererSource -PathType Leaf)) {
    Fail "missing $RendererSource"
}
if (-not (Test-Path -LiteralPath $KotlinWrapper -PathType Leaf)) {
    Fail "missing $KotlinWrapper" @(
        "The checked-in Kotlin Toolchain wrapper is required; no separate Gradle install is used."
    )
}
# This carries the whole verified Compose desktop AWT stack, not just the sun.awt.windows
# classes. An earlier Windows-only selection kept the entries whose names mention Windows and
# dropped the rest, which removed the JNI registration of java.awt.Toolkit.getDefaultToolkit.
# Toolkit.initIDs looks that method up through JNI before any window exists, so the image
# built and then died at startup with NoSuchMethodError on a method the JDK plainly has.
# The reference project hit the identical error with no metadata at all and fixed it with
# this bundle. Entries naming classes this checkout's newer Compose and Skiko no longer have
# are left unresolved by native-image rather than failing the build.
#
# Two directories because native-image documents a configuration directory as holding either
# reachability-metadata.json or the older split files, not both, and the resource and bundle
# declarations are still in the older form.
$ReachabilityMetadata = Join-Path $MetadataDir "reachability-metadata.json"
$ResourceMetadataDir = Join-Path $MetadataDir "resources"
$ResourceMetadata = Join-Path $ResourceMetadataDir "resource-config.json"
$MetadataEvidence = Join-Path $MetadataDir "evidence.json"
if (-not (Test-Path -LiteralPath $ReachabilityMetadata -PathType Leaf) -or
    -not (Test-Path -LiteralPath $ResourceMetadata -PathType Leaf) -or
    -not (Test-Path -LiteralPath $MetadataEvidence -PathType Leaf)) {
    Fail "Windows GraalVM 25 reachability metadata is missing from $MetadataDir" @(
        "Do not replace it with an unattended tracing-agent run.",
        "The agent records only exercised paths and can produce an image that ignores clicks."
    )
}

$GraalHome = $env:GRAALVM_HOME
if ([string]::IsNullOrWhiteSpace($GraalHome)) {
    $GraalHome = $env:JAVA_HOME
}
if ([string]::IsNullOrWhiteSpace($GraalHome)) {
    Fail "GRAALVM_HOME is not set" @(
        "Install Liberica NIK 25 Full, then set GRAALVM_HOME.",
        "The same toolchain builds every desktop platform. Upstream GraalVM works here too,",
        "but it carries AWT as a dozen JDK DLLs the application has to ship beside it, and",
        "NIK carries the same libraries as static archives that link into the image."
    )
}
if (-not (Test-Path -LiteralPath $GraalHome -PathType Container)) {
    Fail "GRAALVM_HOME=$GraalHome does not exist"
}
$NativeImage = Join-Path $GraalHome "bin\native-image.cmd"
if (-not (Test-Path -LiteralPath $NativeImage -PathType Leaf)) {
    $NativeImage = Join-Path $GraalHome "bin\native-image.exe"
}
if (-not (Test-Path -LiteralPath $NativeImage -PathType Leaf)) {
    Fail "GRAALVM_HOME=$GraalHome has no bin\native-image.cmd or bin\native-image.exe" @(
        "Install GraalVM for JDK 25, not a stock JDK."
    )
}
$VersionText = (& $NativeImage --version 2>&1 | Out-String)
if ($LASTEXITCODE -ne 0 -or $VersionText -notmatch "native-image 25\.") {
    Fail "the Windows metadata is pinned to GraalVM 25, but native-image reported: $($VersionText.Trim())" @(
        "Use GraalVM for JDK 25 or recollect and review a version-matched metadata bundle."
    )
}

$VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $VsWhere -PathType Leaf)) {
    Fail "Visual Studio Installer's vswhere.exe was not found" @(
        "Install Visual Studio 2022 Build Tools with Desktop development with C++."
    )
}
$VsInstall = (& $VsWhere -latest -products * `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath | Select-Object -First 1)
if ([string]::IsNullOrWhiteSpace($VsInstall)) {
    Fail "MSVC x64 build tools were not found" @(
        "Install Visual Studio 2022 Build Tools with Desktop development with C++."
    )
}
$VsDevCmd = Join-Path $VsInstall "Common7\Tools\VsDevCmd.bat"
if (-not (Test-Path -LiteralPath $VsDevCmd -PathType Leaf)) {
    Fail "missing $VsDevCmd"
}
$WindowsSdkIncludes = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\Include"
if (-not (Test-Path -LiteralPath $WindowsSdkIncludes -PathType Container)) {
    Fail "Windows 10/11 SDK headers were not found" @(
        "Add a Windows SDK through the Visual Studio Installer."
    )
}

# Import the x64 developer environment so both our C compile and native-image use the same
# MSVC and Windows SDK. Recent native-image can locate MSVC itself, but cl.exe cannot compile
# renderer_entry.c from a plain PowerShell session without INCLUDE and LIB.
$DevEnvironment = & $env:ComSpec /d /s /c "`"$VsDevCmd`" -no_logo -arch=x64 -host_arch=x64 >nul && set"
if ($LASTEXITCODE -ne 0) {
    Fail "VsDevCmd.bat failed to initialise the x64 compiler environment"
}
foreach ($line in $DevEnvironment) {
    $separator = $line.IndexOf("=")
    if ($separator -gt 0) {
        [Environment]::SetEnvironmentVariable(
            $line.Substring(0, $separator),
            $line.Substring($separator + 1),
            "Process"
        )
    }
}
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    Fail "cl.exe is still unavailable after running $VsDevCmd"
}

New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null
$OldJavaHome = $env:JAVA_HOME
$OldGraalHome = $env:GRAALVM_HOME
$OldAutoExit = $env:DIOXUS_COMPOSE_AUTOEXIT_MS
try {
    $env:JAVA_HOME = $GraalHome
    $env:GRAALVM_HOME = $GraalHome
    $env:DIOXUS_COMPOSE_AUTOEXIT_MS = "1"
    # The Kotlin CLI looks for project.yaml in the working directory and its parents, so it
    # has to be run from the Amper project rather than from wherever the build was started.
    # Pointing at the wrapper by full path is not enough: it found no project and said so.
    Push-Location $ProjectDir
    try {
        # The JVM prints its settings on stderr. `2>&1` turns each of those lines into an
        # ErrorRecord, and Tee-Object writes objects through PowerShell's formatter, which
        # is not the text the JVM emitted: the classpath lines came out unparseable. Force
        # every record back to its own string and write the file directly.
        $probe = Invoke-Native {
            & $KotlinWrapper run -m desktop --no-compose-hot-reload `
                "--jvm-args=-XshowSettings:properties" 2>&1
        } | ForEach-Object { $_.ToString() }
        Set-Content -LiteralPath $JvmLog -Value $probe -Encoding UTF8
        $probe | Write-Host
    } finally {
        Pop-Location
    }
    if ($LASTEXITCODE -ne 0) {
        Fail "the JVM classpath probe failed" @(
            "See $JvmLog",
            "The Kotlin CLI must run from $ProjectDir, which holds project.yaml."
        )
    }
} finally {
    $env:JAVA_HOME = $OldJavaHome
    $env:GRAALVM_HOME = $OldGraalHome
    $env:DIOXUS_COMPOSE_AUTOEXIT_MS = $OldAutoExit
}

$ClasspathEntries = [System.Collections.Generic.List[string]]::new()
$Collecting = $false
foreach ($line in Get-Content -LiteralPath $JvmLog) {
    if ($line -match '^\s*java\.class\.path = (.*)$') {
        $ClasspathEntries.Add($Matches[1])
        $Collecting = $true
        continue
    }
    if ($Collecting -and $line -match '^\s{8,}(\S.*)$') {
        $ClasspathEntries.Add($Matches[1])
        continue
    }
    if ($Collecting) {
        break
    }
}
$Classpath = $ClasspathEntries -join [IO.Path]::PathSeparator
if ([string]::IsNullOrWhiteSpace($Classpath)) {
    # Show what was actually captured. Reading this from a log the build then deletes, on
    # a machine nobody has, is how the previous two failures here cost a round trip each.
    [Console]::Error.WriteLine("--- first 40 lines of $JvmLog ---")
    Get-Content -LiteralPath $JvmLog -TotalCount 40 |
        ForEach-Object { [Console]::Error.WriteLine($_) }
    [Console]::Error.WriteLine("--- end ---")
    Fail "could not read java.class.path from $JvmLog" @(
        "The JVM prints its settings on stderr, so the capture has to keep them as text.",
        "A line should read '    java.class.path = <first entry>' with the rest indented."
    )
}
Set-Content -LiteralPath $ClasspathFile -Value $Classpath -NoNewline

Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $DistDir, $ObjDir
New-Item -ItemType Directory -Force -Path $BinDir, $LibDir, $ObjDir | Out-Null
$RendererObject = Join-Path $ObjDir "renderer_entry.obj"
Invoke-Native { & cl.exe /nologo /c /O2 /std:c11 "/Fo$RendererObject" $RendererSource }
if ($LASTEXITCODE -ne 0) {
    Fail "MSVC could not compile $RendererSource"
}

# PE/COFF requires the Host boundary to resolve at DLL link time. renderer_entry.obj supplies
# forwarding definitions that use GetProcAddress on the host executable. Only the two public
# Renderer functions are exported from the DLL.
# The JDK's own desktop libraries, linked in rather than shipped beside the image.
#
# Upstream GraalVM's Windows AWT path emits a dozen DLLs next to the library (awt, jawt,
# java, jvm, fontmanager, freetype, lcms, javajpeg, jsound, javaaccessbridge, mlib_image,
# splashscreen) and every application has to carry all of them. NIK ships the same
# libraries as static archives under lib\static, which is what the macOS build already
# force-loads, so the same thing can be done here and the DLLs stop existing.
#
# Forced rather than linked normally, for the reason the macOS build records: a JNI entry
# point is reached by name at run time and nothing in the image refers to it by symbol, so
# ordinary archive semantics drop the member that defines it.
$StaticDir = Join-Path $GraalHome "lib\static\windows-amd64"
if (-not (Test-Path -LiteralPath $StaticDir -PathType Container)) {
    Fail "no static JDK libraries at $StaticDir" @(
        "This is what NIK has and upstream GraalVM does not.",
        "GRAALVM_HOME=$GraalHome looks like an upstream GraalVM rather than Liberica NIK."
    )
}
# What AWT reaches, and nothing else. Java Sound is not on the list: it wants MIDI and
# DirectSound from the Windows SDK, the link fails on `__imp_midiInOpen` and
# `DirectSoundCreate`, and a renderer that draws a window has no use for either. The same
# reasoning applies to anything else that turns out to need a system library: ask whether
# the renderer uses it before reaching for the import library.
#
# A missing one fails at link time with the symbol named, which is a safe way to be wrong.
# An extra one costs size and can fail like Java Sound did.
$StaticAwtLibraries = @(
    "awt.lib", "jawt.lib", "java.lib", "fontmanager.lib", "freetype.lib", "lcms.lib",
    "javajpeg.lib", "javaaccessbridge.lib", "mlib_image.lib"
)
# The Windows libraries those archives call into.
#
# A DLL carries its own imports, so nothing had to say this while AWT arrived as
# awt.dll. A static archive does not: every Win32 call in it becomes an unresolved symbol
# for whoever links it, and the first attempt failed with 162 of them, from GDI drawing
# (Arc, Ellipse, StrokePath) through common controls (SetWindowSubclass) to printing and
# the common dialogs.
#
# This is the set OpenJDK itself links libawt, fontmanager and the accessibility bridge
# against. Naming a library that turns out to be unnecessary costs nothing at run time,
# because an import library only pulls in what is referenced; missing one fails at link
# time with the symbol named, which is a safe way to be wrong.
$WindowsSystemLibraries = @(
    "gdi32.lib", "user32.lib", "kernel32.lib", "advapi32.lib", "comctl32.lib",
    "comdlg32.lib", "shell32.lib", "shlwapi.lib", "ole32.lib", "oleaut32.lib",
    "uuid.lib", "winspool.lib", "imm32.lib", "msimg32.lib", "winmm.lib", "delayimp.lib"
)

$StaticLinkerArgs = @()
foreach ($Name in $WindowsSystemLibraries) {
    $StaticLinkerArgs += "-H:NativeLinkerOption=$Name"
}
foreach ($Name in $StaticAwtLibraries) {
    $Path = Join-Path $StaticDir $Name
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Fail "missing $Path" @("NIK 25 Full is the distribution that carries these.")
    }
    # The MSVC linker's whole-archive switch, which is what -force_load spells on macOS.
    $StaticLinkerArgs += "-H:NativeLinkerOption=/WHOLEARCHIVE:$Path"
}

$NativeImageArgs = @(
    "--shared",
    "-cp", $Classpath,
    "-o", $LibraryName,
    "--no-fallback",
    "--features=dioxus.compose.ui.platform.ImeReachabilityFeature",
    "--features=dioxus.compose.ui.platform.AccessibilityReachabilityFeature",
    "-Djava.awt.headless=false",
    "-H:IncludeLocales=en,ko",
    "-Os",
    "-H:+UnlockExperimentalVMOptions",
    "-H:ConfigurationFileDirectories=$MetadataDir,$ResourceMetadataDir",
    # The JDK half of the desktop stack, registered wholesale for reflection and JNI.
    # Curated metadata got the image past Toolkit.getDefaultToolkit and straight into the
    # next reflective lookup: Swing asks UIManager for a ComponentUI by class name, and a
    # look and feel class nobody references is not in the image, so a window cannot build
    # its own root pane. Chasing that one class at a time costs a CI run each. This is the
    # recipe Native Image uses for its own non-headless desktop image, and it registers
    # for JNI as well as reflection, so it covers both failures at once.
    "-H:NativeLinkerOption=$RendererObject",
    "-H:NativeLinkerOption=/EXPORT:dioxus_compose_renderer_run",
    "-H:NativeLinkerOption=/EXPORT:dioxus_compose_renderer_request_frame"
) + $StaticLinkerArgs
# Through an argument file, not the command line. The runtime classpath alone is tens of
# kilobytes of Maven cache paths and Windows caps a command line at 32767 characters, so
# passing it directly fails with "The command line is too long." before native-image runs.
# Java argument files treat a backslash inside quotes as an escape, so the paths go in with
# forward slashes, which every Windows API accepts.
$ArgumentFile = Join-Path $BuildDir "native-image-args.txt"
Set-Content -Path $ArgumentFile -Encoding ASCII -Value (
    $NativeImageArgs | ForEach-Object {
        $argument = $_ -replace '\\', '/'
        if ($argument -match '\s') { '"' + $argument + '"' } else { $argument }
    }
)

Push-Location $BinDir
try {
    Invoke-Native { & $NativeImage "@$ArgumentFile" }
    if ($LASTEXITCODE -ne 0) {
        Fail "native-image failed to build the Windows renderer"
    }
} finally {
    Pop-Location
}

$SkikoJar = $ClasspathEntries | Where-Object {
    (Split-Path -Leaf $_) -like "skiko-awt-runtime-windows-x64-*.jar"
} | Select-Object -First 1
if ([string]::IsNullOrWhiteSpace($SkikoJar) -or -not (Test-Path -LiteralPath $SkikoJar)) {
    Fail "no skiko-awt-runtime-windows-x64 jar was found on the runtime classpath" @(
        "Check $ClasspathFile and the compose dependency in native\module.yaml."
    )
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$Archive = [IO.Compression.ZipFile]::OpenRead($SkikoJar)
try {
    foreach ($Name in @("skiko-windows-x64.dll", "icudtl.dat")) {
        $Entry = $Archive.Entries | Where-Object { $_.Name -eq $Name } | Select-Object -First 1
        if ($null -eq $Entry) {
            Fail "$Name is missing from $SkikoJar"
        }
        [IO.Compression.ZipFileExtensions]::ExtractToFile(
            $Entry, (Join-Path $BinDir $Name), $true
        )
    }
} finally {
    $Archive.Dispose()
}

# AWT reads font configuration from <java.home>\lib. RuntimeLayout.kt sets java.home to
# dist because the renderer lives in dist\bin, so stage the JDK 25 files in dist\lib.
$FontFiles = @("fontconfig.bfc", "fontconfig.properties.src", "psfont.properties.ja", "psfontj2d.properties")
foreach ($Name in $FontFiles) {
    $Source = Join-Path $GraalHome "lib\$Name"
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        Fail "required AWT font configuration is missing: $Source"
    }
    Copy-Item -LiteralPath $Source -Destination (Join-Path $LibDir $Name)
}

# What an application actually has to carry. The JDK's desktop DLLs are not in this list
# any more: they are linked into the image from NIK's static archives, the same way the
# macOS build has always done it. Anything still emitted beside the library is checked for
# below and reported, because a DLL appearing here again means the static link silently
# stopped working and every application would start shipping it without anyone deciding to.
$RuntimeFiles = @(
    "$LibraryName.dll", "$LibraryName.lib", "skiko-windows-x64.dll", "icudtl.dat"
)
foreach ($Name in $RuntimeFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $BinDir $Name) -PathType Leaf)) {
        Fail "native-image did not stage $Name in $BinDir" @(
            "This is unverified output naming. Record the directory listing before changing the script."
        )
    }
}

# The JDK DLLs the static link is supposed to have made unnecessary. If one is here, the
# link did not take, and the difference is invisible until an application ships a folder
# twelve files bigger than it should be.
$ShouldBeLinkedIn = @(
    "awt.dll", "jawt.dll", "java.dll", "jvm.dll", "fontmanager.dll", "freetype.dll",
    "lcms.dll", "javajpeg.dll", "javaaccessbridge.dll", "mlib_image.dll", "splashscreen.dll"
)
$StillDynamic = $ShouldBeLinkedIn | Where-Object { Test-Path -LiteralPath (Join-Path $BinDir $_) -PathType Leaf }
if ($StillDynamic) {
    Fail "the JDK desktop libraries were emitted as DLLs: $($StillDynamic -join ', ')" @(
        "They are meant to be linked in from NIK's lib\static\windows-amd64 archives.",
        "Either GRAALVM_HOME is an upstream GraalVM rather than NIK, or the whole-archive",
        "link arguments above stopped reaching the linker."
    )
}

# Keep headers and diagnostic reports out of the runtime bin directory.
$IncludeDir = Join-Path $DistDir "include"
New-Item -ItemType Directory -Force -Path $IncludeDir | Out-Null
Get-ChildItem -LiteralPath $BinDir -Filter "*.h" -File | Move-Item -Destination $IncludeDir
Get-ChildItem -LiteralPath $BinDir -Filter "*.md" -File | Remove-Item

Get-ChildItem -LiteralPath $BinDir, $LibDir
Write-Host "UNTESTED build complete. Verify the window and click path with:"
Write-Host "  powershell -ExecutionPolicy Bypass -File $ScriptsDir\smoke-test-windows.ps1 -RequireClick"
