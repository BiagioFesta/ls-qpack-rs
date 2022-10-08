// Copyright 2022 Biagio Festa

//! Module for decoding operations.
//!
//! The main struct of this module is [`Decoder`].
//!
//! # Example
//!
//! ## Only Static Table
//! ```
//! use ls_qpack::decoder::Decoder;
//! use ls_qpack::encoder::Encoder;
//! use ls_qpack::StreamId;
//!
//! let hdr_encoded = Encoder::new()
//!     .encode_all(StreamId::new(0), [(":status", "404")])
//!     .unwrap()
//!     .take()
//!     .0;
//!
//! let header = Decoder::new(0, 0)
//!     .decode(StreamId::new(0), hdr_encoded)
//!     .unwrap()
//!     .take();
//!
//! println!("Headers: {:?}", header);
//! ```
use crate::Header;
use crate::StreamId;
use std::collections::hash_map;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::marker::PhantomPinned;
use std::pin::Pin;

/// Error during decoding operations.
pub struct DecoderError;

/// The result of a decode operation.
///
/// Generally, this is function's output for [`Decoder::decode`].
///
/// When header data are decoded,
pub enum DecoderOutput {
    /// The header block has been correctly decoded.
    Done(Vec<Header>),

    /// The deocding stream is blocked.
    /// More data are needed in order to proceed with decoding operation.
    /// Generally, you need to feed the encoder via [`Decoder::feed`].
    BlockedStream,
}

impl DecoderOutput {
    /// If the result is unblocked, it will return `Some(Vec<header>)`.
    /// Otherwise `None`.
    pub fn take(self) -> Option<Vec<Header>> {
        match self {
            Self::Done(v) => Some(v),
            Self::BlockedStream => None,
        }
    }

    /// Checks whether the result is blocked or not.
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::BlockedStream)
    }
}

/// A QPACK decoder.
pub struct Decoder {
    inner: Pin<Box<InnerDecoder>>,
}

impl Decoder {
    /// Creates a new decoder.
    ///
    /// Specify the size of the dynamic table (it might be `0`).
    /// And the max number of blocked streams.
    pub fn new(dyn_table_size: u32, max_blocked_streams: u32) -> Self {
        Self {
            inner: InnerDecoder::new(dyn_table_size, max_blocked_streams),
        }
    }

    /// Decodes header data.
    ///
    /// It produces an output, see [`DecoderOutput`].
    ///
    /// It might happen that the data provided to this method are not sufficient in order
    /// to complete the decoding operation.
    /// In that case, more data are needed from the encoder stream (via [`Decoder::feed`]).
    ///
    /// # Examples
    /// ```
    /// use ls_qpack::decoder::Decoder;
    /// use ls_qpack::StreamId;
    ///
    /// # use ls_qpack::TryIntoHeader;
    /// # let (data, _) = ls_qpack::encoder::Encoder::new().encode_all(0.into(), [("foo", "bar")]).unwrap().into();
    ///
    ///
    /// let mut decoder = Decoder::new(0, 0);
    /// let output = decoder.decode(StreamId::new(0), data).unwrap();
    /// ```
    pub fn decode<D>(&mut self, stream_id: StreamId, data: D) -> Result<DecoderOutput, DecoderError>
    where
        D: AsRef<[u8]>,
    {
        self.inner
            .as_mut()
            .feed_header_data(stream_id, data.as_ref())
    }

    /// Feeds data from encoder's buffer stream.
    pub fn feed<D>(&mut self, data: D) -> Result<(), DecoderError>
    where
        D: AsRef<[u8]>,
    {
        self.inner.as_mut().feed_encoder_data(data.as_ref())
    }

    /// Checks whether a header block for a `StreamId` has become unblocked.
    ///
    /// # Returns
    ///   * `None` if the `StreamId` has never been fed.
    ///   * `Some` if the `StreamId` produced an [`DecoderOutput`].
    pub fn unblocked(
        &mut self,
        stream_id: StreamId,
    ) -> Option<Result<DecoderOutput, DecoderError>> {
        self.inner.as_mut().process_decoded_data(stream_id)
    }
}

struct InnerDecoder {
    decoder: ls_qpack_sys::lsqpack_dec,
    header_blocks: HashMap<StreamId, Pin<Box<callbacks::HeaderBlockCtx>>>,
    _marker: PhantomPinned,
}

impl InnerDecoder {
    fn new(dyn_table_size: u32, max_blocked_streams: u32) -> Pin<Box<Self>> {
        let mut this = Box::new(Self {
            decoder: ls_qpack_sys::lsqpack_dec::default(),
            header_blocks: HashMap::new(),
            _marker: PhantomPinned,
        });

        unsafe {
            ls_qpack_sys::lsqpack_dec_init(
                &mut this.decoder,
                std::ptr::null_mut(),
                dyn_table_size,
                max_blocked_streams,
                &callbacks::HSET_IF_CALLBACKS,
                0,
            );
        }

        Box::into_pin(this)
    }

    fn feed_header_data(
        self: Pin<&mut Self>,
        stream_id: StreamId,
        data: &[u8],
    ) -> Result<DecoderOutput, DecoderError> {
        let this = unsafe { self.get_unchecked_mut() };

        if this.header_blocks.contains_key(&stream_id) {
            todo!()
        }

        let mut hblock_ctx =
            callbacks::HeaderBlockCtx::new(&mut this.decoder, data.to_vec().into_boxed_slice());

        let encoded_cursor = hblock_ctx.as_ref().encoded_cursor();
        let encoded_cursor_len = encoded_cursor.len();
        let header_block_len = encoded_cursor.len();
        let mut cursor_after = encoded_cursor.as_ptr();

        let result = unsafe {
            ls_qpack_sys::lsqpack_dec_header_in(
                &mut this.decoder,
                hblock_ctx.as_mut().as_mut_ptr() as *mut libc::c_void,
                stream_id.value(),
                header_block_len,
                &mut cursor_after,
                encoded_cursor_len,
                std::ptr::null_mut(),
                &mut 0,
            )
        };

        match result {
            ls_qpack_sys::lsqpack_read_header_status_LQRHS_DONE => {
                debug_assert!(!hblock_ctx.as_ref().is_blocked());
                debug_assert!(!hblock_ctx.as_ref().is_error());

                let hblock_ctx = unsafe { Pin::into_inner_unchecked(hblock_ctx) };
                Ok(DecoderOutput::Done(hblock_ctx.decoded_headers()))
            }

            ls_qpack_sys::lsqpack_read_header_status_LQRHS_BLOCKED => {
                let offset = unsafe {
                    cursor_after.offset_from(hblock_ctx.as_ref().encoded_cursor().as_ptr())
                };

                debug_assert!(offset > 0);

                hblock_ctx.as_mut().advance_cursor(offset as usize);
                hblock_ctx.as_mut().set_blocked(true);
                this.header_blocks.insert(stream_id, hblock_ctx);

                Ok(DecoderOutput::BlockedStream)
            }

            ls_qpack_sys::lsqpack_read_header_status_LQRHS_NEED => unimplemented!(),

            _ => Err(DecoderError),
        }
    }

    fn feed_encoder_data(self: Pin<&mut Self>, data: &[u8]) -> Result<(), DecoderError> {
        let this = unsafe { self.get_unchecked_mut() };

        let result = unsafe {
            ls_qpack_sys::lsqpack_dec_enc_in(&mut this.decoder, data.as_ptr(), data.len())
        };

        if result == 0 {
            Ok(())
        } else {
            Err(DecoderError)
        }
    }

    fn process_decoded_data(
        self: Pin<&mut Self>,
        stream_id: StreamId,
    ) -> Option<Result<DecoderOutput, DecoderError>> {
        let this = unsafe { self.get_unchecked_mut() };

        match this.header_blocks.entry(stream_id) {
            hash_map::Entry::Occupied(hdbk) => {
                if hdbk.get().as_ref().is_blocked() {
                    debug_assert!(!hdbk.get().as_ref().is_error());
                    return Some(Ok(DecoderOutput::BlockedStream));
                }

                let hdbk = hdbk.remove();

                if hdbk.as_ref().is_error() {
                    debug_assert!(!hdbk.as_ref().is_blocked());
                    return Some(Err(DecoderError));
                }

                let hdbk = unsafe { Pin::into_inner_unchecked(hdbk) };
                Some(Ok(DecoderOutput::Done(hdbk.decoded_headers())))
            }

            hash_map::Entry::Vacant(_) => None,
        }
    }
}

impl Drop for InnerDecoder {
    fn drop(&mut self) {
        unsafe { ls_qpack_sys::lsqpack_dec_cleanup(&mut self.decoder) }
    }
}

mod callbacks {
    use crate::header::HeaderError;
    use crate::Header;
    use std::marker::PhantomPinned;
    use std::pin::Pin;

    pub(super) static HSET_IF_CALLBACKS: ls_qpack_sys::lsqpack_dec_hset_if =
        ls_qpack_sys::lsqpack_dec_hset_if {
            dhi_unblocked: Some(dhi_unblocked),
            dhi_prepare_decode: Some(dhi_prepare_decode),
            dhi_process_header: Some(dhi_process_header),
        };

    #[derive(Debug)]
    pub(super) struct HeaderBlockCtx {
        decoder: *mut ls_qpack_sys::lsqpack_dec, /* TODO(bfesta): maybe &mut and leverage rust checks? */
        encoded_data: Box<[u8]>,
        encoded_data_offset: usize,
        decoding_buffer: Vec<u8>,
        header: ls_qpack_sys::lsxpack_header,
        blocked: bool,
        error: bool,
        decoded_headers: Vec<Header>,
        _marker: PhantomPinned,
    }

    impl HeaderBlockCtx {
        pub(super) fn new(
            decoder: *mut ls_qpack_sys::lsqpack_dec,
            encoded_data: Box<[u8]>,
        ) -> Pin<Box<Self>> {
            debug_assert!(!decoder.is_null());

            Box::pin(Self {
                decoder,
                encoded_data,
                encoded_data_offset: 0,
                decoding_buffer: Vec::new(),
                header: Default::default(),
                blocked: false,
                error: false,
                decoded_headers: Default::default(),
                _marker: PhantomPinned,
            })
        }

        pub(super) unsafe fn as_mut_ptr(mut self: Pin<&mut Self>) -> *mut HeaderBlockCtx {
            self.as_mut().get_unchecked_mut()
        }

        pub(super) fn encoded_cursor<'a>(self: Pin<&'a Self>) -> &'a [u8] {
            debug_assert!(self.encoded_data_offset < self.encoded_data.len());
            &self.get_ref().encoded_data[self.encoded_data_offset..]
        }

        pub(super) fn advance_cursor(self: Pin<&mut Self>, offset: usize) {
            debug_assert!(offset <= self.encoded_data.len());
            let this = unsafe { self.get_unchecked_mut() };
            this.encoded_data_offset += offset;
        }

        pub(super) fn set_blocked(self: Pin<&mut Self>, blocked: bool) {
            let this = unsafe { self.get_unchecked_mut() };
            this.blocked = blocked;
        }

        pub(super) fn enable_error(self: Pin<&mut Self>) {
            let this = unsafe { self.get_unchecked_mut() };
            debug_assert!(!this.error);
            this.error = true;
        }

        pub(super) fn is_blocked(self: Pin<&Self>) -> bool {
            self.blocked
        }

        pub(super) fn is_error(self: Pin<&Self>) -> bool {
            self.error
        }

        pub(super) fn decoded_headers(self) -> Vec<Header> {
            self.decoded_headers
        }

        unsafe fn from_void_ptr(ptr: *mut libc::c_void) -> Pin<&'static mut Self> {
            debug_assert!(!ptr.is_null());
            Pin::new_unchecked(&mut *(ptr as *mut _))
        }

        fn reset_header(self: Pin<&mut Self>) {
            let this = unsafe { self.get_unchecked_mut() };
            this.header = Default::default()
        }

        fn resize_header(self: Pin<&mut Self>, space: u16) {
            let this = unsafe { self.get_unchecked_mut() };
            this.decoding_buffer
                .resize(space as usize, Default::default());

            this.header.buf = this.decoding_buffer.as_mut_ptr() as *mut i8;
            this.header.val_len = space;
        }

        fn header_mut(self: Pin<&mut Self>) -> &mut ls_qpack_sys::lsxpack_header {
            let this = unsafe { self.get_unchecked_mut() };
            &mut this.header
        }

        fn process_header(self: Pin<&mut Self>) -> Result<(), HeaderError> {
            let this = unsafe { self.get_unchecked_mut() };

            let header = Header::with_buffer(
                std::mem::take(&mut this.decoding_buffer).into_boxed_slice(),
                this.header.name_offset as usize,
                this.header.name_len as usize,
                this.header.val_offset as usize,
                this.header.val_len as usize,
            )?;

            this.decoded_headers.push(header);

            this.header = Default::default();

            Ok(())
        }
    }

    extern "C" fn dhi_unblocked(hblock_ctx: *mut libc::c_void) {
        let mut hblock_ctx = unsafe { HeaderBlockCtx::from_void_ptr(hblock_ctx) };

        debug_assert!(hblock_ctx.as_ref().is_blocked());
        hblock_ctx.as_mut().set_blocked(false);

        let encoded_cursor = hblock_ctx.as_ref().encoded_cursor();
        let encoded_cursor_len = encoded_cursor.len();
        let mut cursor_after = encoded_cursor.as_ptr();

        let result = unsafe {
            ls_qpack_sys::lsqpack_dec_header_read(
                hblock_ctx.decoder,
                hblock_ctx.as_mut().as_mut_ptr() as *mut libc::c_void,
                &mut cursor_after,
                encoded_cursor_len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        match result {
            ls_qpack_sys::lsqpack_read_header_status_LQRHS_DONE => {}

            ls_qpack_sys::lsqpack_read_header_status_LQRHS_BLOCKED => {
                let offset = unsafe {
                    cursor_after.offset_from(hblock_ctx.as_ref().encoded_cursor().as_ptr())
                };

                debug_assert!(offset > 0);

                hblock_ctx.as_mut().advance_cursor(offset as usize);
                hblock_ctx.as_mut().set_blocked(true);
            }

            ls_qpack_sys::lsqpack_read_header_status_LQRHS_NEED => unimplemented!(),

            _ => {
                hblock_ctx.as_mut().enable_error();
            }
        }
    }

    extern "C" fn dhi_prepare_decode(
        hblock_ctx: *mut libc::c_void,
        header: *mut ls_qpack_sys::lsxpack_header,
        space: libc::size_t,
    ) -> *mut ls_qpack_sys::lsxpack_header {
        let mut hblock_ctx = unsafe { HeaderBlockCtx::from_void_ptr(hblock_ctx) };

        if space > ls_qpack_sys::LSXPACK_MAX_STRLEN as usize {
            todo!()
        }

        let space = space as u16;

        if header.is_null() {
            hblock_ctx.as_mut().reset_header();
        } else {
            assert!(std::ptr::eq(&hblock_ctx.header, header));
            assert!(space > hblock_ctx.header.val_len);
        }

        hblock_ctx.as_mut().resize_header(space);
        hblock_ctx.as_mut().header_mut()
    }

    extern "C" fn dhi_process_header(
        hblock_ctx: *mut libc::c_void,
        header: *mut ls_qpack_sys::lsxpack_header,
    ) -> libc::c_int {
        let hblock_ctx = unsafe { HeaderBlockCtx::from_void_ptr(hblock_ctx) };

        debug_assert!(!hblock_ctx.blocked);
        debug_assert_eq!(header as *const _, &hblock_ctx.header);

        match hblock_ctx.process_header() {
            Ok(()) => 0,
            Err(_) => todo!(),
        }
    }
}

impl Debug for DecoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecoderError").finish()
    }
}

impl Display for DecoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl std::error::Error for DecoderError {}
