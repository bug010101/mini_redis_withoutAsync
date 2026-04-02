use std::collections::HashMap;

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
            _ => Err("unknown command".to_string()),
        }
    }

    /// 输入一个处理好的命令，执行命令
    /// 1、匹配枚举类型
    /// 2、执行对应的数据库操作
    /// 3、返回String类型
    pub fn execute(&self, db: &mut HashMap<String, String>) -> String {
        match self {
            Command::Set(key, value) => {
                db.insert(key.clone(), value.clone());
                "+OK\r\n".to_string()
            },
            Command::Get(key) => {
                match db.get(key) {
                    Some(v) => format!("${}\r\n{}\r\n", v.len(), v),
                    None => "$-1\r\n".to_string(),
                }
            },
            Command::Del(key) => {
                if db.remove(key).is_some() {
                    ":1\r\n".to_string()
                } else {
                    ":0\r\n".to_string()
                }
            },
            Command::Exists(key) => {
                if db.contains_key(key) {
                    ":1\r\n".to_string()
                } else {
                    ":0\r\n".to_string()
                }
            },
            Command::Incr(key) => {
                Command::execute_number(db, key, 1)
            }
            Command::Decr(key) => {
                Command::execute_number(db, key, -1)
            }
            Command::Incrby(key, increment) => {
                match increment.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, value)
                    },
                    Err(_) => format!("-ERR value is not an integer or out of range\r\n"),
                }
            },
            Command::Decrby(key, decrement ) => {
                match decrement.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, -value)
                    },
                    Err(_) => format!("-ERR value is not an integer or out of range\r\n"),
                }
            },
            Command::Append(key, value) => {
                let current = db.get(key).cloned().unwrap_or_default();
                let next_value = format!("{}{}", current, value);
                let len = next_value.len();
                db.insert(key.to_string(), next_value);
                format!(":{}\r\n", len)
            },
            Command::Strlen(key) => {
                match db.get(key) {
                    Some(v) => format!(":{}\r\n", v.len()),
                    None => format!(":0\r\n")
                }
            },
            Command::Getrange(key, start, end) => {
                match (start.parse::<usize>(), end.parse::<usize>()) {
                    (Ok(start_idx), Ok(end_idx)) => {
                        match db.get(key) {
                            Some(v) => {
                                let bytes = v.as_bytes();
                                let len = bytes.len();
                                
                                // 处理负数索引
                                let start_idx = if start_idx > len { len } else { start_idx };
                                let end_idx = if end_idx >= len { len - 1 } else { end_idx };
                                
                                if start_idx > end_idx {
                                    "$0\r\n\r\n".to_string()
                                } else {
                                    let substring = String::from_utf8_lossy(&bytes[start_idx..=end_idx]);
                                    format!("${}\r\n{}\r\n", substring.len(), substring)
                                }
                            },
                            None => "$-1\r\n".to_string(),
                        }
                    },
                    _ => format!("-ERR value is not an integer or out of range\r\n"),
                }
            },
        }
    }

    fn execute_number(db: &mut HashMap<String, String>, key: &String, increment: i64) -> String {
        let current = db.get(key)
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
        let next_value = current + increment;
        db.insert(key.to_string(), next_value.to_string());
        format!(":{}\r\n", next_value)
    }
}
