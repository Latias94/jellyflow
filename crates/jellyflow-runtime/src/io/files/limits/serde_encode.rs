//! Bounded JSON encoding for persistence files.

use std::io::{self, Write};

use serde::Serialize;

use super::serde_budget::validate_json_depth_with_limits;
use super::{
    BoundedJsonDecodeError, PersistenceLimitError, PersistenceLimits, PersistenceResource,
};

pub(crate) enum BoundedJsonEncodeError {
    Serialize(serde_json::Error),
    Limit(PersistenceLimitError),
}

pub(crate) fn serialize_json_pretty_with_limits<T>(
    value: &T,
    limits: PersistenceLimits,
) -> Result<Vec<u8>, BoundedJsonEncodeError>
where
    T: Serialize + ?Sized,
{
    let mut writer = BoundedVecWriter::new(limits);
    match serde_json::to_writer_pretty(&mut writer, value) {
        Ok(()) => {
            let bytes = writer.into_bytes();
            match validate_json_depth_with_limits(&bytes, limits) {
                Ok(()) => Ok(bytes),
                Err(BoundedJsonDecodeError::Limit(error)) => {
                    Err(BoundedJsonEncodeError::Limit(error))
                }
                Err(BoundedJsonDecodeError::Parse(error)) => {
                    Err(BoundedJsonEncodeError::Serialize(error))
                }
            }
        }
        Err(source) => match writer.take_limit_error() {
            Some(error) => Err(BoundedJsonEncodeError::Limit(error)),
            None => Err(BoundedJsonEncodeError::Serialize(source)),
        },
    }
}

struct BoundedVecWriter {
    bytes: Vec<u8>,
    limits: PersistenceLimits,
    limit_error: Option<PersistenceLimitError>,
}

impl BoundedVecWriter {
    fn new(limits: PersistenceLimits) -> Self {
        const INITIAL_CAPACITY: usize = 8 * 1024;

        Self {
            bytes: Vec::with_capacity(limits.max_bytes().min(INITIAL_CAPACITY)),
            limits,
            limit_error: None,
        }
    }

    fn take_limit_error(&mut self) -> Option<PersistenceLimitError> {
        self.limit_error.take()
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedVecWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let actual = self.bytes.len().saturating_add(buffer.len());
        if let Err(error) = self.limits.ensure(PersistenceResource::FileBytes, actual) {
            self.limit_error = Some(error.clone());
            return Err(io::Error::other(error));
        }

        if actual > self.bytes.capacity() {
            let next_capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(actual)
                .min(self.limits.max_bytes());
            self.bytes
                .try_reserve_exact(next_capacity.saturating_sub(self.bytes.len()))
                .map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
