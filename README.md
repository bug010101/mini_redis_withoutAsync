

# Mini Redis

一个用 Rust 实现的简单 Redis 服务器（异步版本），支持基本 Redis 协议命令。

## 功能特性

### 字符串命令
- `set key value` - 设置键值对
- `get key` - 获取值
- `del key` - 删除键
- `exists key` - 检查键是否存在
- `append key value` - 在指定键的值后面追加内容
- `strlen key` - 获取值的长度
- `getrange key start end` - 获取指定区间的内容

### 数字命令
- `incr key` - 数字值 + 1
- `decr key` - 数字值 - 1
- `incrby key value` - 数字值 + 指定值
- `decrby key value` - 数字值 - 指定值

### 服务命令
- `info` - 获取服务器状态信息

## 环境要求

- Rust 1.56+
- Tokio 异步运行时

## 编译

```bash
# Debug 模式编译
cargo build

# Release 模式编译（推荐生产环境使用）
cargo build --release
```

## 运行

```bash
# 默认监听 localhost:6379
cargo run

# 服务器启动后即可接收客户端连接
```

## 测试

```bash
# 运行所有测试用例
cargo test
```

## 客户端测试示例

使用 `nc` 命令连接服务器进行测试：

```bash
nc localhost 6379
```

### 基础操作

```
set mykey Hello
get mykey
exists mykey
strlen mykey
del mykey
exists mykey
```

### 数字操作

```
set counter 10
incr counter
incrby counter 5
decr counter
decrby counter 3
```

### 字符串操作

```
set greeting Hello
append greeting " World"
getrange greeting 0 4
strlen greeting
info
```

## 项目结构

```
mini_redis_rs/
├── src/
│   ├── command.rs   # 命令解析与执行
│   ├── db.rs       # 数据库存储实现
│   ├── server.rs  # TCP 服务器
│   ├── persistence.rs  # RDB 持久化
│   ├── lib.rs     # 模块导出
│   └── main.rs    # 程序入口
├── tests/
│   └── command_tests.rs  # 命令测试用例
└── Cargo.toml
```

## 技术实现

- **异步网络**: 使用 Tokio 异步运行时处理 TCP 连接
- **数据存储**: 内存 HashMap 存储，支持字符串和整数类型
- **持久化**: RDB 格式定期保存数据到磁盘

## 许可证

MIT License