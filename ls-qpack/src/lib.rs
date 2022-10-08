// Copyright 2022 Biagio Festa

pub use header::Header;
pub use header::TryIntoHeader;
pub use stream_id::StreamId;

pub mod decoder;
pub mod encoder;
mod header;
mod stream_id;

pub mod errors {
    //! Module for errors declarations.

    pub use crate::decoder::DecoderError;
    pub use crate::encoder::EncoderError;
    pub use crate::header::HeaderError;
}
