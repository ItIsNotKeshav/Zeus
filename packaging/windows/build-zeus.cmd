@echo off
rem Builds Zeus's release binary and its Windows installer, as the maintainer's
rem machine does. Needs the Visual Studio 2019 Build Tools, Scoop's perl (Git
rem Bash's perl lacks modules OpenSSL's configure needs) and Inno Setup 6.
rem
rem   packaging\windows\build-zeus.cmd
rem
rem Leaves target-nmake\release\zeus.exe and dist\zeus-<version>-x86_64-setup.exe.
setlocal
cd /d "%~dp0..\.."

call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set "PATH=%USERPROFILE%\scoop\apps\perl\current\perl\bin;%USERPROFILE%\scoop\apps\perl\current\c\bin;%PATH%"
set "CMAKE_GENERATOR=NMake Makefiles"
rem The cache that already holds the C dependencies; another folder starts cold.
set "CARGO_TARGET_DIR=%CD%\target-nmake"

cargo build --release --locked || exit /b 1

rem The package's own version: the first line that starts with it.
for /f "tokens=3" %%v in ('findstr /b /c:"version = " Cargo.toml') do (
  if not defined VERSION set "VERSION=%%~v"
)
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" /Q /DVersion=%VERSION% /DArch=x86_64 ^
  "/DBinary=%CARGO_TARGET_DIR%\release\zeus.exe" "/DOutputDir=%CD%\dist" ^
  packaging\windows\zeus.iss || exit /b 1
echo Built dist\zeus-%VERSION%-x86_64-setup.exe
