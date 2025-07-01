@echo off
echo Building Rust CLI tools for multiple platforms...

:: Check if Rust is installed
cargo --version >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo Rust/Cargo is not installed or not in the PATH.
    exit /b 1
)

:: Create output directories
mkdir dist\linux >nul 2>&1
mkdir dist\windows >nul 2>&1
mkdir dist\macos >nul 2>&1

echo Building for native platform...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo Failed to build for native platform
    exit /b 1
)

echo Building for Windows (x86_64)...
cargo build --release --target x86_64-pc-windows-msvc
if %ERRORLEVEL% NEQ 0 (
    echo Failed to build for Windows
    exit /b 1
)

:: Copy Windows binaries
copy target\x86_64-pc-windows-msvc\release\guid.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\touch.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\cat.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\ts.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\csvn.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\get-lines.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\jwt.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\split.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\eh-read.exe dist\windows\ >nul
copy target\x86_64-pc-windows-msvc\release\eh-export.exe dist\windows\ >nul

echo Build completed successfully.
echo Binaries are available in the dist\ directory.
exit /b 0 