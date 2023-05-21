// Copyright 2022 Biagio Festa

use std::convert::TryFrom;
use std::fmt::Debug;

/// Error during Header construction.
pub enum HeaderError {
    NameTooLong,
    ValueTooLong,
    NameInvalidUtf8,
    ValueInvalidUtf8,
}

/// QPACK Header.
///
/// A `Header` essentially is a pair composed by two strings. That is, a name and a value.
pub struct Header {
    buffer: Box<[u8]>,
    name_offset: usize,
    name_len: usize,
    value_offset: usize,
    value_len: usize,
}

impl Header {
    pub const MAX_LEN_NAME: usize = u16::MAX as usize;
    pub const MAX_LEN_VALUE: usize = u16::MAX as usize;

    /// Builds a new header from a name and a string.
    ///
    /// # Example
    /// ```
    /// use ls_qpack::Header;
    ///
    /// let header = Header::new("name", "bob");
    /// ```
    pub fn new<N, V>(name: N, value: V) -> Result<Self, HeaderError>
    where
        N: AsRef<str>,
        V: AsRef<str>,
    {
        let name = name.as_ref();
        let value = value.as_ref();

        if name.len() > Self::MAX_LEN_NAME {
            return Err(HeaderError::NameTooLong);
        }

        if value.len() > Self::MAX_LEN_VALUE {
            return Err(HeaderError::ValueTooLong);
        }

        let mut buffer = Vec::with_capacity(name.len() + value.len());
        buffer.extend(name.as_bytes());
        buffer.extend(value.as_bytes());

        let name_offset = 0;
        let name_len = name.len();
        let value_offset = name.len();
        let value_len = value.len(); // TODO(bfesta): what if foffset + len?

        Ok(Self {
            buffer: buffer.into_boxed_slice(),
            name_offset,
            name_len,
            value_offset,
            value_len,
        })
    }

    /// Returns the `name` field.
    #[inline]
    pub fn name(&self) -> &str {
        debug_assert!(std::str::from_utf8(
            &self.buffer[self.name_offset..self.name_offset + self.name_len]
        )
        .is_ok());

        unsafe {
            std::str::from_utf8_unchecked(
                &self.buffer[self.name_offset..self.name_offset + self.name_len],
            )
        }
    }

    /// Returns the `value` field.
    #[inline]
    pub fn value(&self) -> &str {
        debug_assert!(std::str::from_utf8(
            &self.buffer[self.value_offset..self.value_offset + self.value_len]
        )
        .is_ok());

        unsafe {
            std::str::from_utf8_unchecked(
                &self.buffer[self.value_offset..self.value_offset + self.value_len],
            )
        }
    }

    pub(crate) fn with_buffer(
        buffer: Box<[u8]>,
        name_offset: usize,
        name_len: usize,
        value_offset: usize,
        value_len: usize,
    ) -> Result<Self, HeaderError> {
        if name_len > Self::MAX_LEN_NAME {
            return Err(HeaderError::NameTooLong);
        }

        if value_len > Self::MAX_LEN_VALUE {
            return Err(HeaderError::ValueTooLong);
        }

        if std::str::from_utf8(&buffer[name_offset..name_offset + name_len]).is_err() {
            return Err(HeaderError::NameInvalidUtf8);
        }

        if std::str::from_utf8(&buffer[value_offset..value_offset + value_len]).is_err() {
            return Err(HeaderError::ValueInvalidUtf8);
        }

        debug_assert!(name_offset < u16::MAX as usize);
        debug_assert!(value_offset < u16::MAX as usize);

        Ok(Self {
            buffer,
            name_offset,
            name_len,
            value_offset,
            value_len,
        })
    }

    // TODO(bfesta): useless function?
    pub(crate) fn build_lsxpack_header(&mut self) -> LsxpackHeader {
        LsxpackHeader::new(self)
    }
}

impl Debug for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Header")
            .field("name", &self.name())
            .field("value", &self.value())
            .finish()
    }
}

/// Tries to convert something into a [`Header`].
///
/// # Examples
/// ```
/// use ls_qpack::TryIntoHeader;
///
/// let header = ("name", "bob").try_into_header();
/// ```
pub trait TryIntoHeader {
    fn try_into_header(self) -> Result<Header, HeaderError>;
}

impl<N, V> TryFrom<(N, V)> for Header
where
    N: AsRef<str>,
    V: AsRef<str>,
{
    type Error = HeaderError;

    fn try_from(value: (N, V)) -> Result<Self, Self::Error> {
        Header::new(value.0, value.1)
    }
}

impl<N, V> TryIntoHeader for (N, V)
where
    N: AsRef<str>,
    V: AsRef<str>,
{
    fn try_into_header(self) -> Result<Header, HeaderError> {
        Header::new(self.0, self.1)
    }
}

pub(crate) struct LsxpackHeader<'a> {
    header_sys: ls_qpack_sys::lsxpack_header,
    #[allow(unused)]
    header_ref: &'a mut Header,
}

impl<'a> LsxpackHeader<'a> {
    pub(crate) fn new(header: &'a mut Header) -> Self {
        debug_assert!(header.name_offset < u16::MAX as usize);
        debug_assert!(header.name_len < Header::MAX_LEN_NAME);
        debug_assert!(header.value_offset < u16::MAX as usize);
        debug_assert!(header.value_len < Header::MAX_LEN_VALUE);

        let header_sys = ls_qpack_sys::lsxpack_header {
            buf: header.buffer.as_mut_ptr() as *mut i8,
            name_offset: header.name_offset as i32,
            name_len: header.name_len as u16,
            val_offset: header.value_offset as i32,
            val_len: header.value_len as u16,
            ..Default::default()
        };

        Self {
            header_sys,
            header_ref: header,
        }
    }
}

impl<'a> AsRef<ls_qpack_sys::lsxpack_header> for LsxpackHeader<'a> {
    fn as_ref(&self) -> &ls_qpack_sys::lsxpack_header {
        &self.header_sys
    }
}
