#!/usr/bin/env bash

# macOS Installation Script for Rust Tools (Fixed & Improved)
#
# Usage:
#   chmod +x install-macos.sh && ./install-macos.sh
#   Or via curl: --update this--
#     curl -sSL https://raw.githubusercontent.com/brenordv/rusted-toolbox/main/install-macos.sh | bash
#
# This script will:
# 1. Check/install Xcode CLI tools and Git
# 2. Check/install Homebrew
# 3. Check/install Rust via rustup
# 4. Clone/update the repository in $CLONE_BASE
# 5. Build the project via build.sh
# 6. Install generated tools to $INSTALL_DIR and update PATH

set -euo pipefail
IFS=$'\n\t'

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
REPO_URL="https://github.com/brenordv/rusted-toolbox.git"
REPO_NAME="rusted-toolbox"
INSTALL_DIR="$HOME/.local/bin"
CLONE_BASE="$HOME/projects"

# Helper printers
print_status()  { echo -e "${BLUE}[INFO]${NC}  $1"; }
print_success() { echo -e "${GREEN}[OK]${NC}    $1"; }
print_warning() { echo -e "${YELLOW}[WARN]${NC}  $1"; }
print_error()   { echo -e "${RED}[ERROR]${NC} $1"; }

# Check whether a command exists
command_exists() { command -v "$1" &>/dev/null; }

# Create installation directory
create_install_dir() {
  if [ ! -d "$INSTALL_DIR" ]; then
    print_status "Creating install directory: $INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"
    print_success "Directory created"
  fi
}

# Idempotently add INSTALL_DIR to user's shell rc
update_path() {
  local shell_name rc_file export_line
  shell_name=$(basename "${SHELL:-/bin/bash}")
  rc_file="$HOME/.${shell_name}rc"
  export_line="export PATH=\"\$PATH:$INSTALL_DIR\""

  if ! grep -qxF "$export_line" "$rc_file" 2>/dev/null; then
    print_status "Adding $INSTALL_DIR to PATH in $rc_file"
    echo "$export_line" >> "$rc_file"
    export PATH="$PATH:$INSTALL_DIR"
    print_warning "Please run 'source $rc_file' or restart your shell to apply PATH changes"
  else
    print_success "PATH already updated in $rc_file"
  fi
}

# Check/install Rust via rustup
check_rust() {
  if ! command_exists rustc || ! command_exists cargo; then
    print_status "Rust not found. Installing with rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
    print_success "Rust installed"
  else
    print_success "Rust is already installed ($(rustc --version))"
  fi
}

# Clone or update repository
setup_repository() {
  mkdir -p "$CLONE_BASE"
  cd "$CLONE_BASE"
  if [ -d "$REPO_NAME" ]; then
    print_status "Updating existing repo: $REPO_NAME"
    cd "$REPO_NAME"
    # stash local changes
    if ! git diff --quiet || ! git diff --cached --quiet; then
      print_warning "Stashing local changes"
      git stash push -m "Auto-stash before update $(date)"
    fi
    git pull origin main || git pull origin master
    print_success "Repository updated"
  else
    print_status "Cloning repo: $REPO_URL"
    git clone "$REPO_URL"
    cd "$REPO_NAME"
    print_success "Repository cloned"
  fi
}

# Build project
build_project() {
  if [ ! -x ./build.sh ]; then
    if [ -f build.sh ]; then
      chmod +x build.sh
    else
      print_error "build.sh not found"
      exit 1
    fi
  fi
  print_status "Running build.sh"
  ./build.sh
  print_success "Build succeeded"
}

# Install built tools
install_tools() {
  local dist_dir="./dist/macos"
  if [ ! -d "$dist_dir" ]; then
    print_error "Distribution directory $dist_dir missing"
    exit 1
  fi
  print_status "Installing tools from $dist_dir to $INSTALL_DIR"
  for tool in "$dist_dir"/*; do
    if [ -f "$tool" ] && [ -x "$tool" ]; then
      cp "$tool" "$INSTALL_DIR/"
      print_success "Installed $(basename "$tool")"
    fi
  done
}

# Main
main() {
  print_status "Starting installer..."

  # Ensure macOS
  if [[ "$OSTYPE" != darwin* ]]; then
    print_error "This script is for macOS only"
    exit 1
  fi

  check_rust
  create_install_dir
  setup_repository
  build_project
  install_tools
  update_path

  print_success "All done! Tools are in $INSTALL_DIR"
}

main "$@"