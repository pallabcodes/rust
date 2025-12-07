//! Reading and writing with buffered I/O using in-memory buffers.

use std::io::{BufRead, BufReader, Write};

pub fn io_demo() -> String {
    let mut output: Vec<u8> = Vec::new();
    writeln!(output, "line one").unwrap();
    writeln!(output, "line two").unwrap();

    let mut reader = BufReader::new(output.as_slice());
    let mut collected = Vec::new();
    let mut buf = String::new();
    while reader.read_line(&mut buf).unwrap() > 0 {
        collected.push(buf.trim_end().to_string());
        buf.clear();
    }

    format!("lines: {:?}", collected)
}

