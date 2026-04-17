#!/bin/bash
set -e

echo "🔧 Installing kiro-cli-auth..."

# Check if kiro-cli is installed
if ! command -v kiro-cli &> /dev/null; then
    echo "❌ kiro-cli is not installed"
    echo "   Please install kiro-cli first from: https://github.com/aws/kiro-cli"
    exit 1
fi

# Determine install location
if [ "$EUID" -eq 0 ]; then
    INSTALL_PATH="/usr/local/bin/kiro-cli-auth"
    echo "📦 Installing system-wide..."
else
    INSTALL_PATH="$HOME/.local/bin/kiro-cli-auth"
    mkdir -p "$HOME/.local/bin"
    echo "📦 Installing to user directory..."
    echo "   (Run with sudo for system-wide installation)"
fi

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux)
        case "$ARCH" in
            x86_64) BINARY="kiro-cli-auth-linux-x86_64" ;;
            aarch64|arm64) BINARY="kiro-cli-auth-linux-aarch64" ;;
            *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    darwin)
        case "$ARCH" in
            x86_64) BINARY="kiro-cli-auth-macos-x86_64" ;;
            arm64) BINARY="kiro-cli-auth-macos-aarch64" ;;
            *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        echo "❌ Unsupported OS: $OS"
        exit 1
        ;;
esac

echo "📥 Downloading $BINARY..."
DOWNLOAD_URL="https://github.com/911218sky/kiro-cli-auth/releases/latest/download/$BINARY"
TEMP_FILE="/tmp/kiro-cli-auth"

if command -v curl &> /dev/null; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_FILE"
elif command -v wget &> /dev/null; then
    wget -q "$DOWNLOAD_URL" -O "$TEMP_FILE"
else
    echo "❌ curl or wget required"
    exit 1
fi

# Install
echo "📦 Installing to $INSTALL_PATH..."
mv "$TEMP_FILE" "$INSTALL_PATH"
chmod +x "$INSTALL_PATH"

# Verify
if command -v kiro-cli-auth &> /dev/null; then
    VERSION=$(kiro-cli-auth --version 2>&1 || echo "unknown")
    echo "✅ Installation successful!"
    echo "   Version: $VERSION"
else
    echo "⚠️  Installed but not in PATH"
    if [ "$EUID" -ne 0 ]; then
        echo ""
        echo "Add to your PATH by adding this to ~/.bashrc or ~/.zshrc:"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
fi

echo ""
echo "🎉 Done! Run: kiro-cli-auth"
