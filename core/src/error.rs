// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

use std::fmt;

/// Errors produced by the decode engine. Corrupt or truncated input always surfaces as an
/// error value; decoding never panics on malformed files.
#[derive(Debug)]
pub enum Error {
    /// The bytes could not be decoded as the detected or declared format.
    Decode(String),
    /// The input is not an image format this build supports.
    Unsupported(String),
    /// Caller-supplied configuration is invalid (rename pattern, job settings, ...).
    Invalid(String),
    /// Reading or writing a file failed.
    Io(std::io::Error),
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Decode(msg) => write!(f, "decode error: {msg}"),
            Error::Unsupported(msg) => write!(f, "unsupported image: {msg}"),
            Error::Invalid(msg) => write!(f, "invalid input: {msg}"),
            Error::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<image::ImageError> for Error {
    fn from(err: image::ImageError) -> Self {
        match err {
            image::ImageError::Unsupported(e) => Error::Unsupported(e.to_string()),
            image::ImageError::IoError(e) => Error::Io(e),
            other => Error::Decode(other.to_string()),
        }
    }
}
