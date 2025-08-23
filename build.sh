#!/bin/bash

echo "Building Rust CLI tools for multiple platforms..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Rust/Cargo is not installed or not in PATH."
    exit 1
fi

# Create output directories
mkdir -p dist

echo "Building for Linux (x86_64)..."
cargo build --release
if [ $? -ne 0 ]; then
    echo "Failed to build for Linux"
    exit 1
fi

# Copy Linux binaries, except touch and cat because you already got that.
find /target/release -maxdepth 1 -type f -executable \
   ! -name "touch" ! -name "cat" \
   -exec cp {} dist/ \;


echo "Build completed successfully for native system."
echo "Binaries are available in the dist/ directory." 