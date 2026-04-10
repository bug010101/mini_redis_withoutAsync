use crate::db::{BaseDb, Db};

#[derive(Debug)]
pub enum Command {
    Set(String, String),
    Get(String),
    Del(String),
    Exists(String),
    Incr(String),
    Decr(String),
    Incrby(String, String),
    Decrby(String, String),
    Append(String, String),
    Strlen(String),
    Getrange(String, String, String),
    Info,
    Exit,
    Ping,
}


impl Command {
    /// 解析&str类型的命令，返回Result<Command, String>类型
    /// 1、将&str根据空格划分为数组
    /// 2、对数组进行切片获取每部分，对比获取对应的枚举类型
    pub fn from_str(line: &str) -> Result<Self, String> {
        let parts:Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["set", key, value] => Ok(Command::Set(key.to_string(), value.to_string())),
            ["get", key] => Ok(Command::Get(key.to_string())),
            ["del", key] => Ok(Command::Del(key.to_string())),
            ["exists", key] => Ok(Command::Exists(key.to_string())),
            ["incr", key] => Ok(Command::Incr(key.to_string())),
            ["decr", key] => Ok(Command::Decr(key.to_string())),
            ["incrby", key, value] => Ok(Command::Incrby(key.to_string(), value.to_string())),
            ["decrby", key, value] => Ok(Command::Decrby(key.to_string(), value.to_string())),
            ["append", key, value] => Ok(Command::Append(key.to_string(), value.to_string())),
            ["strlen", key] => Ok(Command::Strlen(key.to_string())),
            ["getrange", key, start, end] => Ok(Command::Getrange(key.to_string(), start.to_string(), end.to_string())),
            ["info"] => Ok(Command::Info),
            ["exit"] => Ok(Command::Exit),
            ["ping"] => Ok(Command::Ping),
            _ => Err("unknown command".to_string()),
        }
    }

    /// 输入一个处理好的命令，执行命令
    /// 1、匹配枚举类型
    /// 2、执行对应的数据库操作
    /// 3、返回String, bool类型
    pub async fn execute(&self, db: &Db) -> (String, bool) {
        match self {
            Command::Set(key, value) => {
                Self::write_operation(db, |db_write| {
                    db_write.insert(key.to_string(), value.to_string());
                    ("+OK\r\n".to_string(), true)
                }).await
            },
            Command::Get(key) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(v) => (format!("${}\r\n{}\r\n", v.len(), v), false),
                        None => ("$-1\r\n".to_string(), false),
                    }
                }).await
            },
            Command::Del(key) => {
                Self::write_operation(db, |db_del| {
                    if db_del.remove(key).is_some() {
                        (":1\r\n".to_string(), true)
                    } else {
                        (":0\r\n".to_string(), false)
                    }
                }).await
            },
            Command::Exists(key) => {
                Self::read_operation(db, |db_exists| {
                    if db_exists.contains_key(key) {
                        (":1\r\n".to_string(), false)
                    } else {
                        (":0\r\n".to_string(), false)
                    }
                }).await
            },
            Command::Incr(key) => {
                Command::execute_number(db, key, 1).await
            }
            Command::Decr(key) => {
                Command::execute_number(db, key, -1).await
            }
            Command::Incrby(key, increment) => {
                match increment.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, value).await
                    },
                    Err(_) => (format!("-ERR value is not an integer or out of range\r\n"), false),
                }
            },
            Command::Decrby(key, decrement ) => {
                match decrement.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, -value).await
                    },
                    Err(_) => (format!("-ERR value is not an integer or out of range\r\n"), false),
                }
            },
            Command::Append(key, value) => {
                Self::write_operation(db, |db_write| {
                    // let current = db_write.get(key).cloned().unwrap_or_default();
                    // let next_value = format!("{}{}", current, value);
                    let s = db_write.entry(key.clone()).or_insert_with(String::new);
                    s.push_str(value);
                    let len = s.len();
                    (format!(":{}\r\n", len), true)
                }).await
            },
            Command::Strlen(key) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(v) => (format!(":{}\r\n", v.len()), false),
                        None => (format!(":0\r\n"), true)
                    }
                }).await
            },
            Command::Getrange(key, start, end) => {
                Self::read_operation(db, |db_read| {
                    match (start.parse::<usize>(), end.parse::<usize>()) {
                        (Ok(start_idx), Ok(end_idx)) => {
                            match db_read.get(key) {
                                Some(v) => {
                                    let bytes = v.as_bytes();
                                    let len = bytes.len();
                                    // 处理负数索引
                                    let start_idx = if start_idx > len { len } else { start_idx };
                                    let end_idx = if end_idx >= len { len - 1 } else { end_idx };
                                    if start_idx > end_idx {
                                        ("$0\r\n\r\n".to_string(), false)
                                    } else {
                                        let substring = String::from_utf8_lossy(&bytes[start_idx..=end_idx]);
                                        (format!("${}\r\n{}\r\n", substring.len(), substring), false)
                                    }
                                },
                                None => ("$-1\r\n".to_string(), false),
                            }
                        },
                        _ => (format!("-ERR value is not an integer or out of range\r\n"), false),
                    }
                }).await
            },
            Command::Info => {
                // 不用闭包，用守卫操作
                let read_guard = db.read().await;
                let info_content = format!(
                    "# Server\r\n\
                    version:0.1.0\r\n\
                    os:{}\r\n\
                    # Keyspace\r\n\
                    db0:keys={},expires=0\r\n",
                    std::env::consts::OS, 
                    read_guard.len()
                );
                (format!("${}\r\n{}\r\n", info_content.len(), info_content), false)
            },
            Command::Ping => {
                ("+Pong\r\n".to_string(), false)
            }
            Command::Exit => {
                ("+Ok\r\n".to_string(), false)
            },
        }
    }

    async fn execute_number(db: &Db, key: &String, increment: i64) -> (String, bool) {
        Self::write_operation(db, |db_map| {
            let current = db_map.get(key)
                        .and_then(|v| v.parse::<i64>().ok())
                        .unwrap_or(0);
            let next_value = current + increment;
            db_map.insert(key.to_string(), next_value.to_string());
            (format!(":{}\r\n", next_value), true)
        }).await
    }

    async fn read_operation<F, T>(db: &Db, f: F) -> T
    where 
        F: FnOnce(&BaseDb) -> T
    {
        let guard = db.read().await;
        f(&*guard)
    }

    async fn write_operation<F, T>(db: &Db, f: F) -> T
    where 
        F: FnOnce(&mut BaseDb) -> T
    {
        let mut guard = db.write().await;
        f(&mut *guard)
    }
}
