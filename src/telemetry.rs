use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_subscriber::{
    EnvFilter, Layer, Registry, fmt, fmt::writer::MakeWriter, layer::SubscriberExt,
    util::SubscriberInitExt,
};

enum LogWriter {
    File(Arc<Mutex<std::fs::File>>),
    Sink,
}

impl Clone for LogWriter {
    fn clone(&self) -> Self {
        match self {
            LogWriter::File(f) => LogWriter::File(Arc::clone(f)),
            LogWriter::Sink => LogWriter::Sink,
        }
    }
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            LogWriter::File(m) => m
                .lock()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "mutex poisoned"))?
                .write(buf),
            LogWriter::Sink => Ok(buf.len()),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            LogWriter::File(m) => m
                .lock()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "mutex poisoned"))?
                .flush(),
            LogWriter::Sink => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for LogWriter {
    type Writer = LogWriter;
    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}

fn timestamp_filename() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();

    let days = total_secs / 86400;
    let time_secs = total_secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let (year, month, day) = civil_from_days(days as i64);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}-{minutes:02}-{seconds:02}.log")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

/// Initializes the global tracing subscriber with file-only output.
///
/// Logs are written to `<log_dir>/<timestamp>.log` with no ANSI
/// formatting. Console output is suppressed.
pub fn init_telemetry(config_level: &str, log_dir: PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(&log_dir)?;

    let crate_name = env!("CARGO_PKG_NAME").replace('-', "_");

    let filter = if let Ok(rust_log) = std::env::var("RUST_LOG") {
        EnvFilter::new(format!("{rust_log},warn,{crate_name}={}", config_level))
    } else {
        EnvFilter::new(format!("warn,{crate_name}={}", config_level))
    };

    let log_path = log_dir.join(timestamp_filename());
    let log_writer = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(f) => LogWriter::File(Arc::new(Mutex::new(f))),
        Err(_) => LogWriter::Sink,
    };

    let file_layer = fmt::Layer::new()
        .with_writer(log_writer)
        .with_ansi(false)
        .with_filter(filter);

    Registry::default().with(file_layer).init();

    Ok(())
}
