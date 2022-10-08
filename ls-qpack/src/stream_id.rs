// Copyright 2022 Biagio Festa

/// A QUIC stream identifier.
///
/// A wrapper to `u64`.
///
/// # Examples
/// ```
/// # use ls_qpack::StreamId;
/// let stream_id = StreamId::new(64);
/// assert_eq!(stream_id.value(), 64);
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamId(u64);

impl StreamId {
    pub fn new(stream_id: u64) -> Self {
        Self(stream_id)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for StreamId {
    fn from(stream_id: u64) -> Self {
        Self::new(stream_id)
    }
}

impl From<StreamId> for u64 {
    fn from(stream_id: StreamId) -> Self {
        stream_id.0
    }
}
