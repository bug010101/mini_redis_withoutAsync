use std::time::{Duration, Instant};

use crate::{db::{BaseDb, Db, Entry}, protocol::Frame};

#[derive(Debug)]
pub enum Command {
    Set(String, String, Option<u64>), // 增加可选的秒数
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
    Expire(String, u64),
}


impl Command {
    pub fn from_frames(frames: Vec<Frame>) -> Result<Self, String> {
        let mut parts = Vec::new();
        for frame in frames {
            match frame {
                // RESP 命令通常由 Bulk String 组成
                Frame::Bulk(s) | Frame::Simple(s) => parts.push(s),
                _ => return Err("Protocol error: expected bulk string in array".to_string()),
            }
        }
        if parts.is_empty() {
            return Err("Empty command".to_string());
        }
        // 核心：仅将第一个元素（命令名）转为小写
        let command_name = parts[0].to_lowercase();
        let args = &parts[1..];

        match (command_name.as_str(), args) {
            ("set", [key, value]) => Ok(Command::Set(key.clone(), value.clone(), None)),
            ("set", [key, value, ex, secs]) if ex.to_lowercase() == "ex" => {
                let s = secs.parse::<u64>().map_err(|_| "invalid expire time")?;
                Ok(Command::Set(key.clone(), value.clone(), Some(s)))
            },
            ("get", [key]) => Ok(Command::Get(key.clone())),
            ("del", [key]) => Ok(Command::Del(key.clone())),
            ("exists", [key]) => Ok(Command::Exists(key.clone())),
            ("incr", [key]) => Ok(Command::Incr(key.clone())),
            ("decr", [key]) => Ok(Command::Decr(key.clone())),
            ("incrby", [key, val]) => Ok(Command::Incrby(key.clone(), val.clone())),
            ("decrby", [key, val]) => Ok(Command::Decrby(key.clone(), val.clone())),
            ("append", [key, val]) => Ok(Command::Append(key.clone(), val.clone())),
            ("strlen", [key]) => Ok(Command::Strlen(key.clone())),
            ("getrange", [key, start, end]) => Ok(Command::Getrange(key.clone(), start.clone(), end.clone())),
            ("expire", [key, secs]) => {
                let s = secs.parse::<u64>().map_err(|_| "invalid expire time")?;
                Ok(Command::Expire(key.clone(), s))
            },
            ("ping", []) => Ok(Command::Ping),
            ("info", []) => Ok(Command::Info),
            ("exit", []) => Ok(Command::Exit),
            _ => Err(format!("unknown command or wrong arguments: {}", command_name)),
        }
    }


    pub async fn execute(&self, db: &Db) -> (Frame, bool) {
        match self {
            Command::Set(key, value, seconds) => {
                Self::write_operation(db, |db_write| {
                    let expires_at = seconds.map(|s| Instant::now() + Duration::from_secs(s));
                    db_write.insert(key.to_string(), Entry { value: value.to_string(), expires_at });
                    (Frame::Simple("OK".to_string()), true)
                }).await
            },
            Command::Get(key) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key);
                            (Frame::Null, true)
                        } else {
                            (Frame::Bulk(entry.value.clone()), false)
                        }
                    } else {
                        (Frame::Null, false)
                    }
                }).await
            },
            Command::Del(key) => {
                Self::write_operation(db, |db_del| {
                    if db_del.remove(key).is_some() {
                        (Frame::Integer(1), true)
                    } else {
                        (Frame::Integer(0), false)
                    }
                }).await
            },
            Command::Exists(key) => {
                Self::write_operation(db, |db_exists| {
                    if let Some(entry) = db_exists.get(key) {
                        if Self::is_expired(entry) {
                            db_exists.remove(key);
                            (Frame::Null, true)
                        } else {
                            (Frame::Integer(1), false)
                        }
                    } else {
                        (Frame::Integer(0), false)
                    }
                }).await
            },
            Command::Incr(key) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                Command::execute_number(db, key, 1).await
            },
            Command::Decr(key) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                Command::execute_number(db, key, -1).await
            }
            Command::Incrby(key, increment) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                match increment.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, value).await
                    },
                    Err(_) => (Frame::Error("value is not an integer or out of range".to_string()), false),
                }
            },
            Command::Decrby(key, decrement ) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                match decrement.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, -value).await
                    },
                    Err(_) => (Frame::Error("value is not an integer or out of range".to_string()), false),
                }
            },
            Command::Append(key, value) => {
                Self::write_operation(db, |db_write| {
                    let s = db_write.entry(key.clone()).or_insert_with(Entry::new);
                    s.value.push_str(value);
                    let len = s.value.len();
                    (Frame::Integer(len as i64), true)
                }).await
            },
            Command::Strlen(key) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(v) => (Frame::Integer(v.value.len() as i64), false),
                        None => (Frame::Integer(0), false)
                    }
                }).await
            },
            Command::Getrange(key, start, end) => {
                Self::read_operation(db, |db_read| {
                    match (start.parse::<usize>(), end.parse::<usize>()) {
                        (Ok(start_idx), Ok(end_idx)) => {
                            match db_read.get(key) {
                                Some(v) => {
                                    let bytes = v.value.as_bytes();
                                    let len = bytes.len();
                                    // 处理负数索引
                                    let start_idx = if start_idx > len { len } else { start_idx };
                                    let end_idx = if end_idx >= len { len - 1 } else { end_idx };
                                    if start_idx > end_idx {
                                        (Frame::Bulk("".to_string()), false)
                                    } else {
                                        let substring = String::from_utf8_lossy(&bytes[start_idx..=end_idx]);
                                        (Frame::Bulk(substring.to_string()), false)
                                    }
                                },
                                None => (Frame::Null, false),
                            }
                        },
                        _ => (Frame::Error("value is not an integer or out of range".to_string()), false),
                    }
                }).await
            },
            Command::Info => {
                // 不用闭包，用守卫操作
                let read_guard = db.read().await;
                // 统计带有过期时间且尚未过期的 key 数量
                let expires_count = read_guard.values()
                    .filter(|e| Self::is_expired(e))
                    .count();

                let info_content = format!(
                    "# Server\r\nversion:0.1.0\r\nos:{}\r\n# Keyspace\r\ndb0:keys={},expires={}\r\n",
                    std::env::consts::OS, 
                    read_guard.len(),
                    expires_count
                );
                (Frame::Bulk(info_content), false)
            },
            Command::Ping => {
                (Frame::Simple("PONG".to_string()), false)
            }
            Command::Exit => {
                (Frame::Simple("OK".to_string()), false)
            },
            Command::Expire(key, seconds) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get_mut(key) {
                        entry.expires_at = Some(Instant::now() + Duration::from_secs(*seconds));
                        (Frame::Integer(1), true)
                    } else {
                        (Frame::Integer(0), false)
                    }
                }).await
            },
        }
    }

    async fn execute_number(db: &Db, key: &String, increment: i64) -> (Frame, bool) {
        Self::write_operation(db, |db_map| {
            // 获取原数据的时间戳
            let instant = db_map.get(key)
                .map_or(None, |entry| entry.expires_at);
            let current = db_map.get(key)
                        .and_then(|v| v.value.parse::<i64>().ok())
                        .unwrap_or(0);
            let next_value = current + increment;
            db_map.insert(key.to_string(), Entry { value: next_value.to_string(), expires_at: instant });
            (Frame::Integer(next_value), true)
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

    // 判断是否过期
    fn is_expired(entry: &Entry) -> bool {
        entry.expires_at.map_or(false, |expired| expired <= Instant::now())
    }
}
