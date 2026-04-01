# Mini Redis

一个用 Rust 实现的简单 Redis 服务器（同步版本）

## 功能
- SET key value - 设置键值对
- GET key - 获取值

## 编译运行
\`\`\`bash
cargo run
\`\`\`

## 测试
\`\`\`bash
echo "set name rust" | nc localhost 6379
echo "get name" | nc localhost 6379
\`\`\`