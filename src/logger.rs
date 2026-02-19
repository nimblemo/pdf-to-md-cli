use std::io::Write;
use std::sync::{Mutex, OnceLock};

static LOG_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

pub fn set_logger(file: std::fs::File) {
    let _ = LOG_FILE.set(Mutex::new(file));
}

pub fn log_msg(msg: &str) {
    if let Some(mutex) = LOG_FILE.get() {
        if let Ok(mut file) = mutex.lock() {
            let _ = writeln!(file, "{}", msg);
            return;
        }
    }
    eprintln!("{}", msg);
}

#[macro_export]
macro_rules! logger {
    ($($arg:tt)*) => {
        $crate::logger::log_msg(&format!($($arg)*));
    };
}
