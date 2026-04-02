use std::{collections::HashMap, io::{BufRead, BufReader, Write}, net::{TcpListener, TcpStream}};

#[derive(Debug)]
enum Command {
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
    fn from_str(line: &str) -> Result<Self, String> {
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
    fn execute(&self, db: &mut HashMap<String, String>) -> String {
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

fn main() -> std::io::Result<()>{
    let mut db: HashMap<String, String> = HashMap::new(); // 生成数据库对象
    let listener = TcpListener::bind("0.0.0.0:6379")?; // 绑定listerner
    // 同步处理每个stream流
    for stream in listener.incoming() {
        let mut stream = stream?;
        let _ = handle_stream(&mut stream, &mut db);
    }
    Ok(())
}

/// 给定TcpStream，数据库
/// 使用BufReader处理连接
/// 1、根据stream连接获取reader对象
/// 2、开启循环，读取每个命令直到没有任何命令输入为止
/// 3、将输入的命令进行处理，判断，解析，执行
/// 4、将处理的命令传回连接返回给客户端
fn handle_stream(stream: &mut TcpStream, db: &mut HashMap<String, String>) -> std::io::Result<()>{
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimed_cmd = line.trim();
        if trimed_cmd.is_empty() {
            continue;
        }
        let response = match Command::from_str(trimed_cmd) {
            Ok(cmd) => cmd.execute(db),
            Err(e) => format!("-ERR {}\r\n", e),
        };
        stream.write_all(response.as_bytes())?;
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_set() {
        let cmd = Command::from_str("set key value").unwrap();
        matches!(cmd, Command::Set(_, _));
    }

    #[test]
    fn test_parse_get() {
        let cmd = Command::from_str("get key").unwrap();
        matches!(cmd, Command::Get(_));
    }

    #[test]
    fn test_parse_del() {
        let cmd = Command::from_str("del key").unwrap();
        matches!(cmd, Command::Del(_));
    }

    #[test]
    fn test_parse_exists() {
        let cmd = Command::from_str("exists key").unwrap();
        matches!(cmd, Command::Exists(_));
    }

    #[test]
    fn test_parse_unknown_command() {
        let result = Command::from_str("unknown key");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "unknown command");
    }

    #[test]
    fn test_execute_set() {
        let mut db = HashMap::new();
        let cmd = Command::Set("name".to_string(), "rust".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "+OK\r\n");
        assert_eq!(db.get("name").unwrap(), "rust");
    }

    #[test]
    fn test_execute_set_overwrite() {
        let mut db = HashMap::new();
        db.insert("name".to_string(), "old".to_string());
        
        let cmd = Command::Set("name".to_string(), "new".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "+OK\r\n");
        assert_eq!(db.get("name").unwrap(), "new");
    }

    #[test]
    fn test_execute_set_multiple_keys() {
        let mut db = HashMap::new();
        
        Command::Set("key1".to_string(), "value1".to_string()).execute(&mut db);
        Command::Set("key2".to_string(), "value2".to_string()).execute(&mut db);
        Command::Set("key3".to_string(), "value3".to_string()).execute(&mut db);
        
        assert_eq!(db.len(), 3);
        assert_eq!(db.get("key1").unwrap(), "value1");
        assert_eq!(db.get("key2").unwrap(), "value2");
        assert_eq!(db.get("key3").unwrap(), "value3");
    }

    #[test]
    fn test_execute_get_exist() {
        let mut db = HashMap::new();
        db.insert("name".to_string(), "rust".to_string());
        
        let cmd = Command::Get("name".to_string());
        let response = cmd.execute(&mut db);
        
        // RESP 格式：$len\r\nvalue\r\n
        assert_eq!(response, "$4\r\nrust\r\n");
    }

    #[test]
    fn test_execute_get_not_found() {
        let mut db = HashMap::new();
        let cmd = Command::Get("notfound".to_string());
        let response = cmd.execute(&mut db);
        
        // RESP 格式：$-1\r\n（表示 nil）
        assert_eq!(response, "$-1\r\n");
    }

    #[test]
    fn test_execute_get_empty_value() {
        let mut db = HashMap::new();
        db.insert("empty".to_string(), "".to_string());
        
        let cmd = Command::Get("empty".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "$0\r\n\r\n");
    }

    #[test]
    fn test_execute_get_after_set() {
        let mut db = HashMap::new();
        
        Command::Set("key".to_string(), "value".to_string()).execute(&mut db);
        let response = Command::Get("key".to_string()).execute(&mut db);
        
        assert_eq!(response, "$5\r\nvalue\r\n");
    }

    #[test]
    fn test_execute_del_exist() {
        let mut db = HashMap::new();
        db.insert("name".to_string(), "rust".to_string());
        
        let cmd = Command::Del("name".to_string());
        let response = cmd.execute(&mut db);
        
        // RESP 格式：:1\r\n（删除成功）
        assert_eq!(response, ":1\r\n");
        assert!(!db.contains_key("name"));
    }

    #[test]
    fn test_execute_del_not_found() {
        let mut db = HashMap::new();
        let cmd = Command::Del("notfound".to_string());
        let response = cmd.execute(&mut db);
        
        // RESP 格式：:0\r\n（不存在或删除失败）
        assert_eq!(response, ":0\r\n");
    }

    #[test]
    fn test_execute_del_multiple() {
        let mut db = HashMap::new();
        db.insert("key1".to_string(), "value1".to_string());
        db.insert("key2".to_string(), "value2".to_string());
        db.insert("key3".to_string(), "value3".to_string());
        
        Command::Del("key1".to_string()).execute(&mut db);
        let response = Command::Del("key2".to_string()).execute(&mut db);
        
        assert_eq!(response, ":1\r\n");
        assert_eq!(db.len(), 1);
        assert!(db.contains_key("key3"));
    }

    #[test]
    fn test_execute_del_same_key_twice() {
        let mut db = HashMap::new();
        db.insert("name".to_string(), "rust".to_string());
        
        let response1 = Command::Del("name".to_string()).execute(&mut db);
        let response2 = Command::Del("name".to_string()).execute(&mut db);
        
        assert_eq!(response1, ":1\r\n");  // 第一次删除成功
        assert_eq!(response2, ":0\r\n");  // 第二次不存在
    }

    #[test]
    fn test_execute_exists_true() {
        let mut db = HashMap::new();
        db.insert("name".to_string(), "rust".to_string());
        
        let cmd = Command::Exists("name".to_string());
        let response = cmd.execute(&mut db);
        
        // RESP 格式：:1\r\n（存在）
        assert_eq!(response, ":1\r\n");
    }

    #[test]
    fn test_execute_exists_false() {
        let mut db = HashMap::new();
        let cmd = Command::Exists("notfound".to_string());
        let response = cmd.execute(&mut db);
        
        // RESP 格式：:0\r\n（不存在）
        assert_eq!(response, ":0\r\n");
    }

    #[test]
    fn test_execute_exists_after_set() {
        let mut db = HashMap::new();
        
        Command::Set("key".to_string(), "value".to_string()).execute(&mut db);
        let response = Command::Exists("key".to_string()).execute(&mut db);
        
        assert_eq!(response, ":1\r\n");
    }

    #[test]
    fn test_execute_exists_after_del() {
        let mut db = HashMap::new();
        db.insert("key".to_string(), "value".to_string());
        
        Command::Del("key".to_string()).execute(&mut db);
        let response = Command::Exists("key".to_string()).execute(&mut db);
        
        assert_eq!(response, ":0\r\n");
    }

    #[test]
    fn test_execute_exists_multiple_keys() {
        let mut db = HashMap::new();
        db.insert("key1".to_string(), "value1".to_string());
        db.insert("key2".to_string(), "value2".to_string());
        
        let exists_key1 = Command::Exists("key1".to_string()).execute(&mut db);
        let exists_key2 = Command::Exists("key2".to_string()).execute(&mut db);
        let exists_key3 = Command::Exists("key3".to_string()).execute(&mut db);
        
        assert_eq!(exists_key1, ":1\r\n");
        assert_eq!(exists_key2, ":1\r\n");
        assert_eq!(exists_key3, ":0\r\n");
    }

    #[test]
    fn test_execute_append_new_key() {
        let mut db = HashMap::new();
        let cmd = Command::Append("message".to_string(), "hello".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, ":5\r\n");
        assert_eq!(db.get("message").unwrap(), "hello");
    }

    #[test]
    fn test_execute_append_existing() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello".to_string());
        
        let cmd = Command::Append("message".to_string(), " world".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, ":11\r\n");
        assert_eq!(db.get("message").unwrap(), "hello world");
    }

    #[test]
    fn test_execute_append_multiple_times() {
        let mut db = HashMap::new();
        
        Command::Append("str".to_string(), "a".to_string()).execute(&mut db);
        Command::Append("str".to_string(), "b".to_string()).execute(&mut db);
        let response = Command::Append("str".to_string(), "c".to_string()).execute(&mut db);
        
        assert_eq!(response, ":3\r\n");
        assert_eq!(db.get("str").unwrap(), "abc");
    }

    #[test]
    fn test_execute_append_empty_string() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello".to_string());
        
        let cmd = Command::Append("message".to_string(), "".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, ":5\r\n");
        assert_eq!(db.get("message").unwrap(), "hello");
    }

    #[test]
    fn test_execute_strlen_exist() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello".to_string());
        
        let cmd = Command::Strlen("message".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, ":5\r\n");
    }

    #[test]
    fn test_execute_strlen_not_found() {
        let mut db = HashMap::new();
        let cmd = Command::Strlen("notfound".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, ":0\r\n");
    }

    #[test]
    fn test_execute_strlen_empty_string() {
        let mut db = HashMap::new();
        db.insert("empty".to_string(), "".to_string());
        
        let cmd = Command::Strlen("empty".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, ":0\r\n");
    }

    #[test]
    fn test_execute_strlen_long_string() {
        let mut db = HashMap::new();
        db.insert("long".to_string(), "this is a very long string".to_string());
        
        let cmd = Command::Strlen("long".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, ":26\r\n");
    }

    #[test]
    fn test_execute_getrange_basic() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello world".to_string());
        
        let cmd = Command::Getrange("message".to_string(), "0".to_string(), "4".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "$5\r\nhello\r\n");
    }

    #[test]
    fn test_execute_getrange_middle() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello world".to_string());
        
        let cmd = Command::Getrange("message".to_string(), "6".to_string(), "10".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "$5\r\nworld\r\n");
    }

    #[test]
    fn test_execute_getrange_single_char() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello".to_string());
        
        let cmd = Command::Getrange("message".to_string(), "0".to_string(), "0".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "$1\r\nh\r\n");
    }

    #[test]
    fn test_execute_getrange_out_of_range() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello".to_string());
        
        let cmd = Command::Getrange("message".to_string(), "10".to_string(), "20".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "$0\r\n\r\n");
    }

    #[test]
    fn test_execute_getrange_not_found() {
        let mut db = HashMap::new();
        let cmd = Command::Getrange("notfound".to_string(), "0".to_string(), "5".to_string());
        let response = cmd.execute(&mut db);
        
        assert_eq!(response, "$-1\r\n");
    }

    #[test]
    fn test_execute_getrange_invalid_index() {
        let mut db = HashMap::new();
        db.insert("message".to_string(), "hello".to_string());
        
        let cmd = Command::Getrange("message".to_string(), "abc".to_string(), "def".to_string());
        let response = cmd.execute(&mut db);
        
        assert!(response.contains("ERR"));
    }

    #[test]
    fn test_workflow_string_operations() {
        let mut db = HashMap::new();
        
        // 1. SET
        let resp1 = Command::Set("greeting".to_string(), "hello".to_string()).execute(&mut db);
        assert_eq!(resp1, "+OK\r\n");
        
        // 2. STRLEN
        let resp2 = Command::Strlen("greeting".to_string()).execute(&mut db);
        assert_eq!(resp2, ":5\r\n");
        
        // 3. APPEND
        let resp3 = Command::Append("greeting".to_string(), " world".to_string()).execute(&mut db);
        assert_eq!(resp3, ":11\r\n");
        
        // 4. STRLEN after APPEND
        let resp4 = Command::Strlen("greeting".to_string()).execute(&mut db);
        assert_eq!(resp4, ":11\r\n");
        
        // 5. GET
        let resp5 = Command::Get("greeting".to_string()).execute(&mut db);
        assert_eq!(resp5, "$11\r\nhello world\r\n");
        
        // 6. GETRANGE
        let resp6 = Command::Getrange("greeting".to_string(), "0".to_string(), "4".to_string()).execute(&mut db);
        assert_eq!(resp6, "$5\r\nhello\r\n");
        
        // 7. GETRANGE middle
        let resp7 = Command::Getrange("greeting".to_string(), "6".to_string(), "10".to_string()).execute(&mut db);
        assert_eq!(resp7, "$5\r\nworld\r\n");
    }

    #[test]
    fn test_workflow_complete_all_commands() {
        let mut db = HashMap::new();
        
        // String operations
        Command::Set("msg".to_string(), "hello".to_string()).execute(&mut db);
        Command::Append("msg".to_string(), " rust".to_string()).execute(&mut db);
        
        // Numeric operations
        Command::Incr("counter".to_string()).execute(&mut db);
        Command::Incrby("counter".to_string(), "4".to_string()).execute(&mut db);
        
        // Key operations
        let exists = Command::Exists("counter".to_string()).execute(&mut db);
        assert_eq!(exists, ":1\r\n");
        
        let strlen = Command::Strlen("msg".to_string()).execute(&mut db);
        assert_eq!(strlen, ":10\r\n");
        
        let getrange = Command::Getrange("msg".to_string(), "0".to_string(), "4".to_string()).execute(&mut db);
        assert_eq!(getrange, "$5\r\nhello\r\n");
        
        // Delete
        let del = Command::Del("msg".to_string()).execute(&mut db);
        assert_eq!(del, ":1\r\n");
        
        let exists_after = Command::Exists("msg".to_string()).execute(&mut db);
        assert_eq!(exists_after, ":0\r\n");
    }
}