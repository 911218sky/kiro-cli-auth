#!/bin/bash
set -e

echo "🔧 Installing kiro-cli-auth..."

# 檢查是否為 root
if [ "$EUID" -ne 0 ]; then 
    echo "❌ Please run with sudo: sudo ./install.sh"
    exit 1
fi

# 檢查是否已編譯
if [ ! -f "target/release/kiro-cli-auth" ]; then
    echo "❌ Binary not found. Please build first:"
    echo "   cargo build --release"
    exit 1
fi

# 安裝到系統路徑
INSTALL_PATH="/usr/local/bin/kiro-cli-auth"
echo "📥 Installing to $INSTALL_PATH..."
cp target/release/kiro-cli-auth "$INSTALL_PATH"
chmod +x "$INSTALL_PATH"

# 驗證安裝
if command -v kiro-cli-auth &> /dev/null; then
    VERSION=$(kiro-cli-auth --version 2>&1 || echo "unknown")
    echo "✅ Installation successful!"
    echo "   Version: $VERSION"
    echo "   Path: $INSTALL_PATH"
else
    echo "⚠️  Installation completed but kiro-cli-auth not found in PATH"
    echo "   You may need to add /usr/local/bin to your PATH"
fi

echo ""
echo "🎉 Done! You can now use: kiro-cli-auth"
