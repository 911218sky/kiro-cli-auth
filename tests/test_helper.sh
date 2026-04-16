#!/bin/bash
set -e

# 顏色輸出
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Kiro CLI Auth Test Helper ===${NC}\n"

# 清理測試環境
cleanup() {
    echo -e "${YELLOW}Cleaning up test environment...${NC}"
    rm -rf /tmp/kiro-test-*
    echo -e "${GREEN}✓ Cleanup complete${NC}\n"
}

# 測試編譯
test_build() {
    echo -e "${YELLOW}Testing build...${NC}"
    cargo build --release
    echo -e "${GREEN}✓ Build successful${NC}\n"
}

# 測試單元測試
test_unit() {
    echo -e "${YELLOW}Running unit tests...${NC}"
    cargo test --lib
    echo -e "${GREEN}✓ Unit tests passed${NC}\n"
}

# 測試整合測試
test_integration() {
    echo -e "${YELLOW}Running integration tests...${NC}"
    cargo test --test integration_test
    echo -e "${GREEN}✓ Integration tests passed${NC}\n"
}

# 測試所有
test_all() {
    echo -e "${YELLOW}Running all tests...${NC}"
    cargo test
    echo -e "${GREEN}✓ All tests passed${NC}\n"
}

# 測試資料庫操作
test_db_operations() {
    echo -e "${YELLOW}Testing database operations...${NC}"
    
    TEST_DIR="/tmp/kiro-test-$$"
    mkdir -p "$TEST_DIR"
    export KIRO_CLI_AUTH_DIR="$TEST_DIR"
    
    # 使用 Rust 測試
    cargo test --lib db::tests
    
    rm -rf "$TEST_DIR"
    echo -e "${GREEN}✓ Database operations test passed${NC}\n"
}

# 測試遷移
test_migration() {
    echo -e "${YELLOW}Testing JSON to SQLite migration...${NC}"
    
    TEST_DIR="/tmp/kiro-test-migration-$$"
    mkdir -p "$TEST_DIR"
    
    # 建立舊的 JSON 檔案
    cat > "$TEST_DIR/registry.json" <<EOF
{
    "version": "1.0.0",
    "accounts": [
        {
            "id": "test-id",
            "alias": "test",
            "email": "test@example.com",
            "provider": "google",
            "snapshot_path": "/tmp/test.sqlite3",
            "created_at": "2024-01-01T00:00:00Z",
            "last_used": null
        }
    ]
}
EOF
    
    # 執行遷移測試
    cargo test --lib migration::tests::test_migration_success
    
    rm -rf "$TEST_DIR"
    echo -e "${GREEN}✓ Migration test passed${NC}\n"
}

# 顯示幫助
show_help() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  build       - Test build"
    echo "  unit        - Run unit tests"
    echo "  integration - Run integration tests"
    echo "  all         - Run all tests"
    echo "  db          - Test database operations"
    echo "  migration   - Test JSON to SQLite migration"
    echo "  cleanup     - Clean up test environment"
    echo "  help        - Show this help message"
    echo ""
}

# 主程式
case "${1:-all}" in
    build)
        test_build
        ;;
    unit)
        test_unit
        ;;
    integration)
        test_integration
        ;;
    all)
        test_build
        test_unit
        test_integration
        ;;
    db)
        test_db_operations
        ;;
    migration)
        test_migration
        ;;
    cleanup)
        cleanup
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        echo -e "${RED}Unknown command: $1${NC}"
        show_help
        exit 1
        ;;
esac

echo -e "${GREEN}=== All tests completed successfully ===${NC}"
