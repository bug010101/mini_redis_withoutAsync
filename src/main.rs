use std::{collections::HashMap, io::{Read, Write}, net::{TcpListener, TcpStream}};


fn main() -> std::io::Result<()>{
    // 先生成数据库对象
    let mut db: HashMap<String, String> = HashMap::new();
    let listener = TcpListener::bind("0.0.0.0:6379")?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        handle_stream(&mut stream, &mut db).unwrap();
    }
    
    Ok(())
}

/// 给定TcpStream，数据库
/// 对数据库进行操作
fn handle_stream(stream: &mut TcpStream, db: &mut HashMap<String, String>) -> std::io::Result<()>{
    let mut buffer = [0u8; 1024]; // 获取缓冲区
    let mut partial = String::new(); // 逐块拼接
    loop {
        let n = stream.read(&mut buffer)?; // 读入缓冲区
        if n == 0 {
            break;
        }
        let cmd = String::from_utf8_lossy(&buffer[..n]); // 获取读入的内容
        partial.push_str(&cmd); // 每次读入的内容都放入拼接块
        // 处理包含多个命令的行
        handle_multi_raw(&mut partial, db, stream);
    }
    Ok(())
}

/// 处理包含多个命令的行
fn handle_multi_raw(partial: &mut String, db: &mut HashMap<String, String>, stream: &mut TcpStream) {
    while let Some(pos) = partial.find('\n') {
        let line = partial[..pos].trim_end_matches(&['\r', '\n'][..]).to_string();
        partial.drain(..=pos); // 删除已经处理的行
        if line.is_empty() {
            continue;
        }
        let response = execute_cmd(db, &line).unwrap();
        stream.write_all(response.as_bytes()).unwrap();
        stream.write_all(b"\r\n").unwrap();
    }
}


/// 输入一个处理好命令，执行命令
/// 将数据进行分割得到集合
/// 对集合进行匹配
/// 进行数据库操作并返回用户提示
fn execute_cmd(db: &mut HashMap<String, String>, line: &String) -> Option<String>{
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["set", key, value] => {
            db.insert(key.to_string(), value.to_string());
            Some("+ADD OK\r\n".to_string())
        },
        ["get", key] => Some(db.get(*key)
                                .cloned()
                                .unwrap_or_else(|| "$-1\r\n".into())),
        _ => Some("-ERR unKnown command\r\n".to_string()),
    }
}