#!/usr/bin/env bash
#
# Ubuntu/Linux Installation Script for Rust Tools
#
# Usage:
#   chmod +x convenience-build-ubuntu.sh && ./convenience-build-ubuntu.sh
#   Or via curl:
#     curl -sSL https://raw.githubusercontent.com/brenordv/rusted-toolbox/refs/heads/master/convenience-build-ubuntu.sh | bash
#
# This script will:
# 1. Check/install Rust via rustup
# 2. Clone/update the repository in $CLONE_BASE
# 3. Build the project for Linux
# 4. Install tools from target/release to $INSTALL_DIR and update PATH
# 5. Exclude 'cat' and 'touch' tools to avoid conflicts with coreutils
#
# Notes:
# - Recommended (optional) packages for common Rust crates on Ubuntu:
#     sudo apt-get update && sudo apt-get install -y build-essential pkg-config libssl-dev
# - rustup docs: https://rustup.rs/

set -euo pipefail
IFS=$'\n\t'

# ---------- Colors ----------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# ---------- Config ----------
REPO_URL="https://github.com/brenordv/rusted-toolbox.git"
REPO_NAME="rusted-toolbox"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CLONE_BASE="${CLONE_BASE:-$HOME/projects}"

# ---------- Helpers ----------
print_status()  { echo -e "${BLUE}[INFO]${NC}  $1"; }
print_success() { echo -e "${GREEN}[OK]${NC}    $1"; }
print_warning() { echo -e "${YELLOW}[WARN]${NC}  $1"; }
print_error()   { echo -e "${RED}[ERROR]${NC} $1"; }

command_exists() { command -v "$1" &>/dev/null; }

# Ensure we're on Linux (preferably Ubuntu, but allow other distros)
ensure_linux() {
  case "$(uname -s | tr '[:upper:]' '[:lower:]')" in
    linux) : ;;
    *)
      print_error "This script targets Linux. Detected: $(uname -s)"
      exit 1
      ;;
  esac

  if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    if [ "${ID:-}" = "ubuntu" ] || [[ "${ID_LIKE:-}" == *debian* ]]; then
      print_success "Linux detected (${PRETTY_NAME:-Ubuntu/Debian-like})"
    else
      print_warning "Non-Ubuntu distro detected (${PRETTY_NAME:-unknown}). Proceeding generically."
    fi
  else
    print_warning "/etc/os-release not found; proceeding on generic Linux."
  fi
}

create_install_dir() {
  if [ ! -d "$INSTALL_DIR" ]; then
    print_status "Creating install directory: $INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"
    print_success "Directory created"
  else
    print_success "Install directory exists: $INSTALL_DIR"
  fi
}

# Idempotently add INSTALL_DIR to user's shell rc
update_path() {
  local shell_name rc_file export_line
  shell_name=$(basename "${SHELL:-/bin/bash}")

  case "$shell_name" in
    bash) rc_file="$HOME/.bashrc" ;;
    zsh)  rc_file="$HOME/.zshrc" ;;
    fish)
      # fish uses a different mechanism; still export for current session
      rc_file="$HOME/.config/fish/config.fish"
      ;;
    *)    rc_file="$HOME/.${shell_name}rc" ;;
  esac

  export_line="export PATH=\"\$PATH:$INSTALL_DIR\""

  if [ ! -f "$rc_file" ]; then
    print_warning "Shell rc file not found ($rc_file). Creating it."
    touch "$rc_file"
  fi

  if ! grep -qxF "$export_line" "$rc_file" 2>/dev/null; then
    print_status "Adding $INSTALL_DIR to PATH in $rc_file"
    echo "$export_line" >> "$rc_file"
    # apply to current session too
    export PATH="$PATH:$INSTALL_DIR"
    print_warning "Run 'source $rc_file' or open a new shell to apply PATH changes"
  else
    print_success "PATH already updated in $rc_file"
  fi
}

# Check/install Rust via rustup
check_rust() {
  if ! command_exists rustc || ! command_exists cargo; then
    if ! command_exists curl; then
      print_error "'curl' is required to install rustup. Install it (e.g., 'sudo apt-get install -y curl') and rerun."
      exit 1
    fi
    print_status "Rust not found. Installing via rustup..."
    # Ref: https://rustup.rs/
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
    print_success "Rust installed ($(rustc --version))"
  else
    print_success "Rust is already installed ($(rustc --version))"
  fi
}

# Clone or update repository
setup_repository() {
  if ! command_exists git; then
    print_error "'git' is required. Install it (e.g., 'sudo apt-get install -y git') and rerun."
    exit 1
  fi

  mkdir -p "$CLONE_BASE"
  cd "$CLONE_BASE"
  if [ -d "$REPO_NAME/.git" ]; then
    print_status "Updating existing repo: $REPO_NAME"
    cd "$REPO_NAME"
    # stash local changes
    if ! git diff --quiet || ! git diff --cached --quiet; then
      print_warning "Stashing local changes"
      git stash push -m "Auto-stash before update $(date)"
    fi
    # try main then master
    if git ls-remote --exit-code --heads origin main &>/dev/null; then
      git pull --ff-only origin main
    else
      git pull --ff-only origin master || true
    fi
    print_success "Repository updated"
  else
    print_status "Cloning repo: $REPO_URL"
    git clone "$REPO_URL"
    cd "$REPO_NAME"
    print_success "Repository cloned"
  fi
}

# Build project for Linux
build_project() {
  print_status "Building project (release)"
  cargo build --release
  print_success "Build succeeded"
}

# Install built tools
install_tools() {
  local release_dir="./target/release"
  if [ ! -d "$release_dir" ]; then
    print_error "Release directory $release_dir missing"
    exit 1
  fi

  print_status "Installing tools from $release_dir to $INSTALL_DIR"

  # Exclude 'cat' and 'touch' to avoid clashes with coreutils
  local tools=(
    "how"
    "aiignore"
    "csvn"
    "eh-export"
    "eh-read"
    "get-lines"
    "gitignore"
    "guid"
    "http"
    "imgx"
    "jwt"
    "mock"
    "split"
    "ts"
    "whisper"
  )

  for tool in "${tools[@]}"; do
    local tool_path="$release_dir/$tool"
    if [ -f "$tool_path" ] && [ -x "$tool_path" ]; then
      cp "$tool_path" "$INSTALL_DIR/"
      print_success "Installed $tool"
    else
      print_warning "Tool $tool not found or not executable at $tool_path"
    fi
  done
}

main() {
  print_status "Starting installer for Linux/Ubuntu..."
  ensure_linux
  check_rust
  create_install_dir
  setup_repository
  build_project
  install_tools
  update_path
  print_success "All done! Tools are in $INSTALL_DIR"
}

main "$@"