use crate::models::{get_aite_config_dir, AppConfig, AITE_APP_CONFIG_FILE};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use tracing_subscriber::fmt::writer::MakeWriter;

pub const DEBUG_LOG_FILE: &str = "debug.log";

fn get_app_config_path() -> PathBuf {
    get_aite_config_dir().join(AITE_APP_CONFIG_FILE)
}

pub fn get_debug_log_path() -> PathBuf {
    get_aite_config_dir().join(DEBUG_LOG_FILE)
}

pub fn is_debug_logging_enabled() -> bool {
    let config_path = get_app_config_path();

    match fs::read_to_string(config_path) {
        Ok(content) if !content.trim().is_empty() => serde_json::from_str::<AppConfig>(&content)
            .map(|config| config.debug_enabled)
            .unwrap_or(true),
        _ => true,
    }
}

#[derive(Clone, Default)]
pub struct DebugFileMakeWriter;

pub struct DebugFileWriter;

impl<'a> MakeWriter<'a> for DebugFileMakeWriter {
    type Writer = DebugFileWriter;

    fn make_writer(&'a self) -> Self::Writer {
        DebugFileWriter
    }
}

impl Write for DebugFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !is_debug_logging_enabled() {
            return Ok(buf.len());
        }

        let log_path = get_debug_log_path();

        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
            let _ = file.write_all(buf);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
