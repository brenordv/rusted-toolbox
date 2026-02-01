# Distro-cc - Distro Command Converter

A CLI tool that translates package manager commands between Linux distributions.

It first tries an internal conversion map (Debian/Ubuntu `apt/apt-get` ↔ Arch `pacman`).
If it can’t convert safely, it falls back to the AI engine (the same configuration used by other AI tools).

## Installation
Build from source using the build scripts in the repository root.

## Usage
```bash
# Convert an apt command to pacman
distro-cc -f debian -t arch -c apt install neovim

# Convert a pacman command to apt
distro-cc -f arch -t debian -c pacman -S htop

# Auto-detect target distro from /etc/os-release
distro-cc -f debian -c apt install ripgrep

# Quiet output (only command)
distro-cc -f debian -t arch -c apt install git -n
```

## Alias Examples
```bash
# Map-native aliases (Arch): run apt/apt-get and get pacman output
alias apt='distro-cc -f debian -t arch -c apt'
alias apt-get='distro-cc -f debian -t arch -c apt-get'
apt install fd

# Map-native aliases (Debian/Ubuntu): run pacman and get apt output
alias pacman='distro-cc -f arch -t debian -c pacman'
pacman -Syu

# Complex aliases that preserve argument order/quoting (bash/zsh)
apt() {
  local cmd
  printf -v cmd 'apt %q ' "$@"
  distro-cc -f debian -t arch -c "$cmd"
}

pacman() {
  local cmd
  printf -v cmd 'pacman %q ' "$@"
  distro-cc -f arch -t debian -c "$cmd"
}
```

## Command Line Options
- `-f`, `--from <DISTRO>` - Source distro for the current command
- `-t`, `--to <DISTRO>` - Target distro (optional; auto-detected on Linux)
- `-c`, `--command <COMMAND>` - Command to be converted
- `-n`, `--no-header` - Suppress header output
- `-v`, `--verbose` - Log conversion steps

## Environment Variables (AI)
This tool uses the same AI configuration as other AI tools via `ai-shared`:

- `AI_PLATFORM`: `openai`, `openrouter`, or `local` (OpenWebUI)
- `OPEN_AI_API_KEY`, `OPEN_AI_MODEL`, `OPEN_AI_API_URL`, `OPEN_AI_ORGANIZATION`
- `OPEN_ROUTER_API_KEY`, `OPEN_ROUTER_MODEL`, `OPEN_ROUTER_API_URL`
- `LOCAL_OPENWEBUI_API_KEY`, `LOCAL_OPENWEBUI_MODEL`, `LOCAL_OPENWEBUI_URL`
- Optional: `*_TEMPERATURE`, `*_CHAT_REQUEST_HISTORY_PATH`

## Caveats
- Package managers are not fully equivalent; flags and package names can differ.
- Commands with unsupported flags fall back to AI and may still be imperfect.
- Auto-detection depends on `/etc/os-release` and may fail in containers or non-Linux OSes.
- Aliases that forward arguments must preserve quoting to avoid argument drift.