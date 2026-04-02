use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use crate::command::Command;
use std::collections::HashMap;
pub fn run_server() -> std::io::Result<()>{
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
