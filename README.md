# Mini Redis

一个用 Rust 实现的简单 Redis 服务器（同步版本）

## 功能
- set key value - 设置键值对
- get key - 获取值
- del key - 删除值
- exists key - 是否存在值
- incr key - key(i64) + 1
- decr key - key(i64) - 1
- incrby key value - key(i64) + value
- decrby key value - key(i64) - value
## 编译运行
```bash
cargo run
```

## 一键测试
```
cargo test
```

## 测试
```
> nc localhost:6379 \
> set key 1 \
> get key \
> del key \
> exists key \
> incr key \
> decr key \
> incrby key 5
> decrby key 5
```