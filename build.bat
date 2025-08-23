@echo off
echo Building Rust CLI tools for Windows...

:: Check if Rust is installed
cargo --version >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo Rust/Cargo is not installed or not in the PATH.
    exit /b 1
)

mkdir dist\windows >nul 2>&1

echo Building for native platform...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo Failed to build for native platform
    exit /b 1
)
:: Copy Windows binaries
copy target\release\*.exe dist\windows\ >nul

echo Build completed successfully.
echo Binaries are available in the dist\ directory.
exit /b 0 