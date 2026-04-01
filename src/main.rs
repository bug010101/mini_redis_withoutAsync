use std::{collections::HashMap, io::{BufRead, BufReader, Write}, net::{TcpListener, TcpStream}};

enum Command {
    Set(String, String),
    Get(String),
    // Del(String),
    // Exists(String),
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
            _ => Err("unknown command".to_string()),
        }
    }

    /// 输入一个处理好的命令，执行命令
    /// 1、匹配枚举类型
    /// 2、执行对应的数据库操作
    /// 3、返回String类型
    fn execute(db: &mut HashMap<String, String>, cmd: &Command) -> String {
        match cmd {
            Command::Set(key, value) => {
                db.insert(key.clone(), value.clone());
                "+OK\r\n".to_string()
            },
            Command::Get(key) => {
                match db.get(key) {
                    Some(v) => format!("${}\r\n{}\r\n", v.len(), v),
                    None => "$-1\r\n".to_string(),
                }
            }
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
            Ok(cmd) => Command::execute(db, &cmd),
            Err(e) => format!("-ERR {}\r\n", e),
        };
        stream.write_all(response.as_bytes())?;
    }
    Ok(())
}
