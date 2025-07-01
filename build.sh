#!/bin/bash

echo "Building Rust CLI tools for multiple platforms..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Rust/Cargo is not installed or not in PATH."
    exit 1
fi

# Create output directories
mkdir -p dist/linux
mkdir -p dist/windows
mkdir -p dist/macos

echo "Building for Linux (x86_64)..."
cargo build --release --target x86_64-unknown-linux-gnu
if [ $? -ne 0 ]; then
    echo "Failed to build for Linux"
    exit 1
fi

# Copy Linux binaries
cp target/x86_64-unknown-linux-gnu/release/guid dist/linux/
cp target/x86_64-unknown-linux-gnu/release/ts dist/linux/
cp target/x86_64-unknown-linux-gnu/release/csvn dist/linux/
cp target/x86_64-unknown-linux-gnu/release/get-lines dist/linux/
cp target/x86_64-unknown-linux-gnu/release/jwt dist/linux/
cp target/x86_64-unknown-linux-gnu/release/split dist/linux/
cp target/x86_64-unknown-linux-gnu/release/eh-read dist/linux/
cp target/x86_64-unknown-linux-gnu/release/eh-export dist/linux/

echo "Building for Windows (x86_64)..."
cargo build --release --target x86_64-pc-windows-gnu
if [ $? -ne 0 ]; then
    echo "Failed to build for Windows"
    exit 1
fi

# Copy Windows binaries
cp target/x86_64-pc-windows-gnu/release/guid.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/touch.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/cat.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/ts.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/csvn.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/get-lines.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/jwt.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/split.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/eh-read.exe dist/windows/
cp target/x86_64-pc-windows-gnu/release/eh-export.exe dist/windows/

echo "Building for macOS (x86_64)..."
cargo build --release --target x86_64-apple-darwin
if [ $? -ne 0 ]; then
    echo "Failed to build for macOS"
    exit 1
fi

# Copy macOS binaries
cp target/x86_64-apple-darwin/release/guid dist/macos/
cp target/x86_64-apple-darwin/release/ts dist/macos/
cp target/x86_64-apple-darwin/release/csvn dist/macos/
cp target/x86_64-apple-darwin/release/get-lines dist/macos/
cp target/x86_64-apple-darwin/release/jwt dist/macos/
cp target/x86_64-apple-darwin/release/split dist/macos/
cp target/x86_64-apple-darwin/release/eh-read dist/macos/
cp target/x86_64-apple-darwin/release/eh-export dist/macos/

echo "Build completed successfully for Linux, Windows, and macOS."
echo "Binaries are available in the dist/ directory." 