#!/bin/bash
# Builds every tool in the workspace and copies the binaries into dist/.
# Tools live under crates/tools/*; shared libraries under crates/libs/*.
# The whole workspace is built in one pass so shared crates compile only once.

echo "Building Rust CLI tools..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Rust/Cargo is not installed or not in PATH."
    exit 1
fi

# Create output directory
mkdir -p dist

echo "Building the workspace (release)..."
cargo build --release
if [ $? -ne 0 ]; then
    echo "Failed to build the workspace"
    exit 1
fi

# Copy every built binary except touch and cat, which the system already provides.
# Binaries are the extension-less files at the top of target/release; .d files are build metadata.
find ./target/release -maxdepth 1 -type f ! -name "*.d" ! -name "touch" ! -name "cat" \
   -exec cp {} dist/ \;

echo "Build completed successfully for native system."
echo "Binaries are available in the dist/ directory."
