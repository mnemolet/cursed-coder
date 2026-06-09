use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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

/// Initializes the global tracing subscriber with dual-layer output:
///
/// - **Terminal** (stderr): events passing the `EnvFilter` with the given
///   `config_level` for this crate and `warn` for dependencies.
/// - **File** (`~/.config/cursedcoder/cursedcoder.log`): all events, no ANSI.
///
/// If `RUST_LOG` is set, it overrides the entire filter string for both layers.
pub fn init_telemetry(config_level: &str, log_dir: PathBuf) -> io::Result<()> {
    std::fs::create_dir_all(&log_dir)?;

    let filter = if let Ok(rust_log) = std::env::var("RUST_LOG") {
        EnvFilter::new(rust_log)
    } else {
        EnvFilter::new(format!("warn,{}={}", env!("CARGO_PKG_NAME"), config_level))
    };

    let stderr_layer = fmt::Layer::new()
        .with_writer(io::stderr)
        .with_filter(filter);

    let log_path = log_dir.join("cursedcoder.log");
    let log_writer = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(f) => LogWriter::File(Arc::new(Mutex::new(f))),
        Err(_) => LogWriter::Sink,
    };

    let file_layer = fmt::Layer::new().with_writer(log_writer).with_ansi(false);

    Registry::default()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    Ok(())
}
