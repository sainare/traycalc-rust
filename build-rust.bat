@echo off
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cargo +stable-x86_64-pc-windows-gnullvm build --release
if errorlevel 1 exit /b %errorlevel%
if not exist dist mkdir dist
copy /y "target\x86_64-pc-windows-gnullvm\release\traycalc.exe" "dist\TrayCalcRuntime.exe" >nul
copy /y "%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnullvm\bin\libunwind.dll" "dist\libunwind.dll" >nul
"%WINDIR%\Microsoft.NET\Framework64\v4.0.30319\csc.exe" /nologo /target:winexe /out:"dist\TrayCalc.exe" /resource:"dist\TrayCalcRuntime.exe",TrayCalcPayload /resource:"dist\libunwind.dll",UnwindPayload SingleFileLauncher.cs
if errorlevel 1 exit /b %errorlevel%
del /q "dist\TrayCalcRuntime.exe" "dist\libunwind.dll"
echo Готово: dist\TrayCalc.exe
