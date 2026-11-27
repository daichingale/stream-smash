@echo off
setlocal
pushd "%~dp0" || exit /b 1
cargo build --release --locked
if errorlevel 1 goto fail
set "csc=%WINDIR%\Microsoft.NET\Framework64\v4.0.30319\csc.exe"
if not exist "%csc%" set "csc=%WINDIR%\Microsoft.NET\Framework\v4.0.30319\csc.exe"
if not exist "%csc%" (
    echo Windows .NET Framework compiler not found.
    goto fail
)
"%csc%" /nologo /codepage:65001 /optimize+ /target:winexe /out:target\release\StreamSmash.exe /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.Web.Extensions.dll /resource:windows\mobile.html,StreamSmash.Mobile windows\Launcher.cs windows\PreviewPane.cs windows\PreviewFeed.cs windows\StudioColors.cs windows\ProcessUsage.cs windows\MobileRemote.cs windows\MobilePairing.cs windows\PairingQr.cs
if errorlevel 1 goto fail
echo Built target\release\StreamSmash.exe
popd
exit /b 0
:fail
popd
exit /b 1
