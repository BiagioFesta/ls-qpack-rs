// Copyright 2022 Biagio Festa

//! Module for encoding operations.
//!
//! The main struct of this module is [`Encoder`].
//!
//! # Examples
//!
//! ## Only Static Table
//! ```
//! use ls_qpack::encoder::Encoder;
//! use ls_qpack::StreamId;
//!
//! let (enc_hdr, enc_stream) = Encoder::new()
//!     .encode_all(
//!         StreamId::new(0),
//!         [(":status", "404"), (":method", "connect")],
//!     )
//!     .unwrap()
//!     .into();
//!
//! // Using only static table. We don't expect stream data.
//! assert_eq!(enc_stream.len(), 0);
//! println!("Encoded data: {:?}", enc_hdr);
//! ```
use crate::header::TryIntoHeader;
use crate::StreamId;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::marker::PhantomPinned;
use std::pin::Pin;

/// Error during encoding operations.
pub struct EncoderError;

/// A QPACK encoder.
pub struct Encoder {
    inner: Pin<Box<InnerEncoder>>,
    seqnos: HashMap<StreamId, u32>,
}

impl Encoder {
    /// Creates a new encoder.
    ///
    /// If not configured, this encoder will only make use of a static table.
    ///
    /// Once peer's settings has been received, you might want to allocate
    /// the dynamic table by means of [`Self::configure`].
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: InnerEncoder::new(),
            seqnos: HashMap::new(),
        }
    }

    /// Sets dynamic table size and it applies peer's settings.
    ///
    /// # Returns
    /// SDTC instruction (Set Dynamic Table Capacity) data. This should be
    /// transmitted to the peer via encoder stream.
    ///
    /// # Notes
    ///   * `dyn_table_size` can be `0` to avoid dynamic table.
    ///   * `dyn_table_size` cannot be larger than `max_table_size`.
    #[inline]
    pub fn configure(
        &mut self,
        max_table_size: u32,
        dyn_table_size: u32,
        max_blocked_streams: u32,
    ) -> Result<SDTCInstruction, EncoderError> {
        self.inner
            .as_mut()
            .init(max_table_size, dyn_table_size, max_blocked_streams)
            .map(SDTCInstruction)
    }

    /// Encodes an entire header block (a list of headers).
    ///
    /// # Returns
    /// The encoded data (see [`BuffersEncoded`]).
    ///
    /// # Examples
    /// ```
    /// use ls_qpack::encoder::Encoder;
    /// use ls_qpack::StreamId;
    ///
    /// let mut encoder = Encoder::new();
    /// let (enc_hdr, enc_stream) = encoder
    ///     .encode_all(
    ///         StreamId::new(0),
    ///         [(":status", "404"), (":method", "connect")],
    ///     )
    ///     .unwrap()
    ///     .into();
    /// ```
    pub fn encode_all<I, H>(
        &mut self,
        stream_id: StreamId,
        headers: I,
    ) -> Result<BuffersEncoded, EncoderError>
    where
        I: IntoIterator<Item = H>,
        H: TryIntoHeader,
    {
        let mut encoding = self.encoding(stream_id);

        for header in headers {
            encoding.append(header)?;
        }

        encoding.encode()
    }

    /// Encodes a list of headers in a sequential fashion way.
    ///
    /// This method is similar to [`Self::encode_all`]. However, instead of
    /// providing the entire list of header, it is possible to append a header step by step.
    ///
    /// See [`EncodingBlock`].
    ///
    /// # Examples
    /// ```
    /// use ls_qpack::encoder::Encoder;
    /// use ls_qpack::StreamId;
    ///
    /// let mut encoder = Encoder::new();
    /// let mut encoding_block = encoder.encoding(StreamId::new(0));
    ///
    /// encoding_block.append((":status", "404"));
    /// encoding_block.append((":method", "connect"));
    ///
    /// let (enc_hdr, enc_stream) = encoding_block.encode().unwrap().into();
    /// ```
    #[inline]
    pub fn encoding(&mut self, stream_id: StreamId) -> EncodingBlock<'_> {
        let seqno = {
            let seqno_ref = self.seqnos.entry(stream_id).or_default();
            std::mem::replace(seqno_ref, seqno_ref.wrapping_add(1))
        };

        EncodingBlock::new(self, stream_id, seqno)
    }

    /// Return estimated compression ratio until this point.
    ///
    /// Compression ratio is defined as size of the output divided by the size of the
    /// input, where output includes both header blocks and instructions sent
    /// on the encoder stream.
    #[inline]
    pub fn ratio(&self) -> f32 {
        self.inner.as_ref().ratio()
    }

    #[inline]
    fn inner_mut(&mut self) -> Pin<&mut InnerEncoder> {
        self.inner.as_mut()
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

/// SDTC instruction
///
/// *Set Dynamic Table Capacity* data.
/// It is a buffer of data to be fed to the peer's decoder.
#[derive(Debug)]
pub struct SDTCInstruction(Box<[u8]>);

impl SDTCInstruction {
    /// Returns the buffer data.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.0
    }

    /// Takes the ownership returning the inner buffer data.
    #[inline]
    pub fn take(self) -> Box<[u8]> {
        self.0
    }
}

impl AsRef<[u8]> for SDTCInstruction {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.data()
    }
}

impl From<SDTCInstruction> for Box<[u8]> {
    fn from(sdtc_instruction: SDTCInstruction) -> Self {
        sdtc_instruction.0
    }
}

/// An encoding operation for a headers block.
///
/// This is the result of [`Encoder::encoding`] method.
pub struct EncodingBlock<'a>(&'a mut Encoder);

impl<'a> EncodingBlock<'a> {
    fn new(encoder: &'a mut Encoder, stream_id: StreamId, seqno: u32) -> Self {
        encoder
            .inner_mut()
            .start_header_block(stream_id, seqno)
            .map(|()| Self(encoder))
            .unwrap() // unwrap is safe here because no other start-block can happen
    }

    /// Appends a header to encode.
    pub fn append<H>(&mut self, header: H) -> Result<&mut Self, EncoderError>
    where
        H: TryIntoHeader,
    {
        self.0.inner_mut().encode(header).map(|()| self)
    }

    /// Encodes the header block.
    pub fn encode(self) -> Result<BuffersEncoded, EncoderError> {
        self.0
            .inner_mut()
            .end_header_block()
            .map(|(header, stream)| BuffersEncoded {
                header: header.into_boxed_slice(),
                stream: stream.into_boxed_slice(),
            })
    }
}

/// The result of the encoding operation.
///
/// This is the result of [`Encoder::encode_all`] or [`EncodingBlock::encode`].
pub struct BuffersEncoded {
    header: Box<[u8]>,
    stream: Box<[u8]>,
}

impl BuffersEncoded {
    /// The data buffer of encoded headers.
    pub fn header(&self) -> &[u8] {
        &self.header
    }

    /// The buffer of the stream data for the decoder.
    pub fn stream(&self) -> &[u8] {
        &self.stream
    }

    pub fn take(self) -> (Box<[u8]>, Box<[u8]>) {
        self.into()
    }
}

impl From<BuffersEncoded> for (Box<[u8]>, Box<[u8]>) {
    fn from(buffers_encoded: BuffersEncoded) -> Self {
        (buffers_encoded.header, buffers_encoded.stream)
    }
}

struct InnerEncoder {
    encoder: ls_qpack_sys::lsqpack_enc,
    enc_buffer: Vec<u8>,
    hdr_buffer: Vec<u8>,
    _marker: PhantomPinned,
}

impl InnerEncoder {
    fn new() -> Pin<Box<Self>> {
        let mut this = Box::new(Self {
            encoder: ls_qpack_sys::lsqpack_enc::default(),
            enc_buffer: Vec::new(),
            hdr_buffer: Vec::new(),
            _marker: PhantomPinned,
        });

        unsafe {
            ls_qpack_sys::lsqpack_enc_preinit(&mut this.encoder, std::ptr::null_mut());
        }

        Box::into_pin(this)
    }

    fn init(
        self: Pin<&mut Self>,
        max_table_size: u32,
        dyn_table_size: u32,
        max_blocked_streams: u32,
    ) -> Result<Box<[u8]>, EncoderError> {
        let this = unsafe { self.get_unchecked_mut() };

        let mut buffer = vec![0; ls_qpack_sys::LSQPACK_LONGEST_SDTC as usize];
        let mut sdtc_buffer_size = buffer.len();

        let result = unsafe {
            ls_qpack_sys::lsqpack_enc_init(
                &mut this.encoder,
                std::ptr::null_mut(),
                max_table_size,
                dyn_table_size,
                max_blocked_streams,
                ls_qpack_sys::lsqpack_enc_opts_LSQPACK_ENC_OPT_STAGE_2,
                buffer.as_mut_ptr(),
                &mut sdtc_buffer_size,
            )
        };

        if result == 0 {
            buffer.truncate(sdtc_buffer_size);
            Ok(buffer.into_boxed_slice())
        } else {
            Err(EncoderError)
        }
    }

    /// Returns error if another block is started before completing the previous.
    fn start_header_block(
        self: Pin<&mut Self>,
        stream_id: StreamId,
        seqno: u32,
    ) -> Result<(), EncoderError> {
        let this = unsafe { self.get_unchecked_mut() };

        let result = unsafe {
            ls_qpack_sys::lsqpack_enc_start_header(&mut this.encoder, stream_id.value(), seqno)
        };

        if result == 0 {
            this.enc_buffer.clear();
            this.hdr_buffer.clear();

            Ok(())
        } else {
            Err(EncoderError)
        }
    }

    fn encode<H>(self: Pin<&mut Self>, header: H) -> Result<(), EncoderError>
    where
        H: TryIntoHeader,
    {
        const BUFFER_SIZE: usize = 1024;

        let mut header = header.try_into_header().map_err(|_| EncoderError)?;

        let this = unsafe { self.get_unchecked_mut() };

        // TODO(bfesta): we want to better handle capacity.
        // In particular, if the encoding op fails because of buffer too small we want report that, at least.

        let enc_buffer_offset = this.enc_buffer.len();
        this.enc_buffer.resize(enc_buffer_offset + BUFFER_SIZE, 0);

        let hdr_buffer_offset = this.hdr_buffer.len();
        this.hdr_buffer.resize(hdr_buffer_offset + BUFFER_SIZE, 0);

        let mut enc_buffer_size = this.enc_buffer.len() - enc_buffer_offset;
        let mut hdr_buffer_size = this.hdr_buffer.len() - hdr_buffer_offset;

        let result = unsafe {
            ls_qpack_sys::lsqpack_enc_encode(
                &mut this.encoder,
                this.enc_buffer.as_mut_ptr().add(enc_buffer_offset),
                &mut enc_buffer_size,
                this.hdr_buffer.as_mut_ptr().add(hdr_buffer_offset),
                &mut hdr_buffer_size,
                header.build_lsxpack_header().as_ref(),
                0,
            )
        };

        if result == ls_qpack_sys::lsqpack_enc_status_LQES_OK {
            this.enc_buffer
                .truncate(enc_buffer_offset + enc_buffer_size);
            this.hdr_buffer
                .truncate(hdr_buffer_offset + hdr_buffer_size);

            Ok(())
        } else {
            this.enc_buffer.truncate(enc_buffer_offset);
            this.hdr_buffer.truncate(hdr_buffer_offset);

            Err(EncoderError)
        }
    }

    /// Finalize the encoded header block.
    ///
    /// It computes the header prefix and return the encoded buffers.
    /// It returns a buffer pair:
    ///   * Buffer of encoded header
    ///   * Buffer of encoded bytes to write on the encoder stream.
    fn end_header_block(self: Pin<&mut Self>) -> Result<(Vec<u8>, Vec<u8>), EncoderError> {
        let this = unsafe { self.get_unchecked_mut() };

        let max_prefix_len =
            unsafe { ls_qpack_sys::lsqpack_enc_header_block_prefix_size(&this.encoder) };

        let mut hdr_block = vec![0; max_prefix_len + this.hdr_buffer.len()];

        let hdr_prefix_len = unsafe {
            ls_qpack_sys::lsqpack_enc_end_header(
                &mut this.encoder,
                hdr_block.as_mut_ptr(),
                max_prefix_len,
                std::ptr::null_mut(),
            )
        };

        if hdr_prefix_len > 0 {
            hdr_block.truncate(hdr_prefix_len as usize);
            hdr_block.extend_from_slice(&this.hdr_buffer);

            Ok((hdr_block, std::mem::take(&mut this.enc_buffer)))
        } else {
            Err(EncoderError)
        }
    }

    fn ratio(self: Pin<&Self>) -> f32 {
        unsafe { ls_qpack_sys::lsqpack_enc_ratio(&self.encoder) }
    }
}

impl Drop for InnerEncoder {
    fn drop(&mut self) {
        unsafe { ls_qpack_sys::lsqpack_enc_cleanup(&mut self.encoder) }
    }
}

impl Debug for EncoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncoderError").finish()
    }
}

impl Display for EncoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl std::error::Error for EncoderError {}

#[cfg(test)]
mod tests {
    use super::Encoder;
    use super::StreamId;

    #[test]
    fn test_encoder_determinism_static() {
        let mut encoder = Encoder::new();

        let results = (0..1024)
            .map(|_| {
                encoder
                    .encode_all(StreamId::new(0), utilities::HEADERS_LIST_1)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        assert!(results.iter().all(|b| b.header() == results[0].header()));
        assert!(results.iter().all(|b| b.stream().is_empty()));
    }

    mod utilities {
        pub(super) const HEADERS_LIST_1: [(&str, &str); 2] =
            [(":status", "404"), (":method", "connect")];
    }
}
