// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Support for stdbuf environment variables in Rust programs.
//!
//! The `stdbuf` utility sets `_STDBUF_I`, `_STDBUF_O`, and `_STDBUF_E` environment
//! variables to control buffering. This module provides writers that respect these
//! settings, allowing Rust programs to be controlled by `stdbuf`.

use std::env;
use std::io::{self, BufWriter, Write};

/// Buffering mode parsed from stdbuf environment variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferMode {
    /// Unbuffered - write immediately without buffering
    Unbuffered,
    /// Line buffered - flush after each newline
    LineBuffered,
    /// Fully buffered with specified size
    FullyBuffered(usize),
    /// Default buffering (no stdbuf override)
    Default,
}

impl BufferMode {
    /// Parse buffering mode from environment variable value.
    ///
    /// Values:
    /// - "0" -> Unbuffered
    /// - "L" -> Line buffered
    /// - numeric -> Fully buffered with that size
    pub fn from_env_value(value: &str) -> Self {
        match value {
            "0" => BufferMode::Unbuffered,
            "L" => BufferMode::LineBuffered,
            s => s
                .parse::<usize>()
                .map(BufferMode::FullyBuffered)
                .unwrap_or(BufferMode::Default),
        }
    }

    /// Get the stdout buffering mode from `_STDBUF_O` environment variable.
    pub fn stdout() -> Self {
        env::var("_STDBUF_O")
            .map(|v| Self::from_env_value(&v))
            .unwrap_or(BufferMode::Default)
    }

    /// Get the stderr buffering mode from `_STDBUF_E` environment variable.
    pub fn stderr() -> Self {
        env::var("_STDBUF_E")
            .map(|v| Self::from_env_value(&v))
            .unwrap_or(BufferMode::Default)
    }
}

/// A writer that flushes after every newline (line-buffered mode).
pub struct LineBufferedWriter<W: Write> {
    inner: W,
}

impl<W: Write> LineBufferedWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<W: Write> Write for LineBufferedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        // Flush if buffer contains a newline
        if buf[..n].contains(&b'\n') {
            self.inner.flush()?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Create a stdout writer that respects stdbuf settings.
///
/// This checks the `_STDBUF_O` environment variable and returns an appropriate writer:
/// - "0" (unbuffered): Returns raw stdout without buffering
/// - "L" (line buffered): Returns a line-buffered writer
/// - numeric: Returns a BufWriter with that capacity
/// - default: Returns a BufWriter with the specified default capacity
pub fn stdout_writer(default_capacity: usize) -> Box<dyn Write> {
    let stdout = io::stdout();

    match BufferMode::stdout() {
        BufferMode::Unbuffered => Box::new(stdout.lock()),
        BufferMode::LineBuffered => Box::new(LineBufferedWriter::new(stdout.lock())),
        BufferMode::FullyBuffered(size) => Box::new(BufWriter::with_capacity(size, stdout.lock())),
        BufferMode::Default => Box::new(BufWriter::with_capacity(default_capacity, stdout.lock())),
    }
}
