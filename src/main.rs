use std::{collections::HashMap, io::{BufRead, BufReader, Write}, net::{TcpListener, TcpStream}};

#[derive(Debug)]
enum Command {
    Set(String, String),
    Get(String),
    Del(String),
    Exists(String),
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
        }
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
    fn test_workflow() {
        let mut db = HashMap::new();
        
        // 1. SET
        let resp1 = Command::Set("user".to_string(), "alice".to_string()).execute(&mut db);
        assert_eq!(resp1, "+OK\r\n");
        
        // 2. EXISTS
        let resp2 = Command::Exists("user".to_string()).execute(&mut db);
        assert_eq!(resp2, ":1\r\n");
        
        // 3. GET
        let resp3 = Command::Get("user".to_string()).execute(&mut db);
        assert_eq!(resp3, "$5\r\nalice\r\n");
        
        // 4. DEL
        let resp4 = Command::Del("user".to_string()).execute(&mut db);
        assert_eq!(resp4, ":1\r\n");
        
        // 5. EXISTS after DEL
        let resp5 = Command::Exists("user".to_string()).execute(&mut db);
        assert_eq!(resp5, ":0\r\n");
        
        // 6. GET after DEL
        let resp6 = Command::Get("user".to_string()).execute(&mut db);
        assert_eq!(resp6, "$-1\r\n");
    }
}