@echo off
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cargo +stable-x86_64-pc-windows-gnullvm build --release
if errorlevel 1 exit /b %errorlevel%
if not exist dist mkdir dist
copy /y "target\x86_64-pc-windows-gnullvm\release\traycalc.exe" "dist\TrayCalc.exe" >nul
copy /y "%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnullvm\bin\libunwind.dll" "dist\libunwind.dll" >nul
echo Готово: dist\TrayCalc.exe и dist\libunwind.dll
