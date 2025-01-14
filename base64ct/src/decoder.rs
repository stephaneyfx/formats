//! Stateful Base64 decoder.

use crate::{
    encoding::decode_padding,
    variant::Variant,
    Encoding,
    Error::{self, InvalidLength},
};
use core::marker::PhantomData;

#[cfg(docsrs)]
use crate::{Base64, Base64Unpadded};

/// Stateful Base64 decoder with support for buffered, incremental decoding.
///
/// The `E` type parameter can be any type which impls [`Encoding`] such as
/// [`Base64`] or [`Base64Unpadded`].
///
/// Internally it uses a sealed `Variant` trait which is an implementation
/// detail of this crate, and leverages a [blanket impl] of [`Encoding`].
///
/// [blanket impl]: ./trait.Encoding.html#impl-Encoding
#[derive(Clone)]
pub struct Decoder<'i, E: Variant> {
    /// Remaining data in the input buffer.
    remaining: &'i [u8],

    /// Block buffer used for non-block-aligned data.
    buffer: BlockBuffer,

    /// Phantom parameter for the Base64 encoding in use.
    encoding: PhantomData<E>,
}

impl<'i, E: Variant> Decoder<'i, E> {
    /// Create a new decoder for a byte slice containing contiguous
    /// (non-newline-delimited) Base64-encoded data.
    pub fn new(input: &'i [u8]) -> Result<Self, Error> {
        let remaining = if E::PADDED {
            // TODO(tarcieri): validate that padding is well-formed with `validate_padding`
            let (unpadded_len, err) = decode_padding(input)?;
            if err != 0 {
                return Err(Error::InvalidEncoding);
            }

            &input[..unpadded_len]
        } else {
            input
        };

        Ok(Self {
            remaining,
            buffer: BlockBuffer::default(),
            encoding: PhantomData,
        })
    }

    /// Fill the provided buffer with data decoded from Base64.
    ///
    /// Enough Base64 input data must remain to fill the entire buffer.
    ///
    /// # Returns
    /// - `Ok(bytes)` if the expected amount of data was read
    /// - `Err(Error::InvalidLength)` if the exact amount of data couldn't be read
    pub fn decode<'o>(&mut self, out: &'o mut [u8]) -> Result<&'o [u8], Error> {
        if self.is_finished() {
            return Err(Error::InvalidLength);
        }

        let mut out_off = 0;

        if !self.buffer.is_empty() {
            let bytes = self.buffer.take(out.len());
            out[..bytes.len()].copy_from_slice(bytes);
            out_off += bytes.len();
        }

        let out_rem = out.len().checked_sub(out_off).ok_or(InvalidLength)?;
        let out_aligned = out_rem.checked_sub(out_rem % 3).ok_or(InvalidLength)?;

        let mut in_len = out_aligned
            .checked_mul(4)
            .and_then(|n| n.checked_div(3))
            .ok_or(InvalidLength)?;

        if in_len > self.remaining.len() {
            return Err(Error::InvalidLength);
        }

        if in_len < 4 {
            in_len = 0;
        }

        let (aligned, rest) = self.remaining.split_at(in_len);

        if in_len != 0 {
            let decoded_len =
                E::Unpadded::decode(aligned, &mut out[out_off..][..out_aligned])?.len();

            out_off = out_off.checked_add(decoded_len).ok_or(InvalidLength)?;
            self.remaining = rest;
        }

        if out_off < out.len() && !self.remaining.is_empty() {
            let (block, rest) = if self.remaining.len() < 4 {
                (self.remaining, [].as_ref())
            } else {
                self.remaining.split_at(4)
            };

            self.buffer.fill::<E::Unpadded>(block)?;
            self.remaining = rest;

            let bytes = self
                .buffer
                .take(out.len().checked_sub(out_off).ok_or(InvalidLength)?);

            out[out_off..][..bytes.len()].copy_from_slice(bytes);
            out_off = out_off.checked_add(bytes.len()).ok_or(InvalidLength)?;
        }

        if out.len() == out_off {
            Ok(out)
        } else {
            Err(InvalidLength)
        }
    }

    /// Has all of the input data been decoded?
    pub fn is_finished(&self) -> bool {
        self.remaining.is_empty() && self.buffer.is_empty()
    }
}

/// Base64 decode buffer for a 1-block input.
///
/// This handles a partially decoded block of data, i.e. data which has been
/// decoded but not read.
#[derive(Clone, Default)]
struct BlockBuffer {
    /// 3 decoded bytes from a 4-byte Base64-encoded input.
    decoded: [u8; 3],

    /// Length of the buffer.
    length: usize,

    /// Position within the buffer.
    position: usize,
}

impl BlockBuffer {
    /// Fill the buffer by decoding up to 4 bytes of Base64 input
    fn fill<E: Variant>(&mut self, base64_input: &[u8]) -> Result<(), Error> {
        debug_assert!(self.is_empty());
        debug_assert!(base64_input.len() <= 4);
        self.length = E::decode(base64_input, &mut self.decoded)?.len();
        self.position = 0;
        Ok(())
    }

    /// Take a specified number of bytes from the buffer.
    ///
    /// Returns as many bytes as possible, or an empty slice if the buffer has
    /// already been read to completion.
    fn take(&mut self, mut nbytes: usize) -> &[u8] {
        debug_assert!(self.position <= self.length);
        let start_pos = self.position;
        let remaining_len = self.length - start_pos;

        if nbytes > remaining_len {
            nbytes = remaining_len;
        }

        self.position += nbytes;
        &self.decoded[start_pos..][..nbytes]
    }

    /// Have all of the bytes in this buffer been consumed?
    pub fn is_empty(&self) -> bool {
        self.position == self.length
    }
}

#[cfg(test)]
mod tests {
    use crate::{Base64, Base64Unpadded, Decoder};

    /// Padded Base64-encoded example
    const PADDED_BASE64: &str =
         "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBHwf2HMM5TRXvo2SQJjsNkiDD5KqiiNjrGVv3UUh+mMT5RHxiRtOnlqvjhQtBq0VpmpCV/PwUdhOig4vkbqAcEc=";
    const PADDED_BIN: &[u8] = &[
        0, 0, 0, 19, 101, 99, 100, 115, 97, 45, 115, 104, 97, 50, 45, 110, 105, 115, 116, 112, 50,
        53, 54, 0, 0, 0, 8, 110, 105, 115, 116, 112, 50, 53, 54, 0, 0, 0, 65, 4, 124, 31, 216, 115,
        12, 229, 52, 87, 190, 141, 146, 64, 152, 236, 54, 72, 131, 15, 146, 170, 138, 35, 99, 172,
        101, 111, 221, 69, 33, 250, 99, 19, 229, 17, 241, 137, 27, 78, 158, 90, 175, 142, 20, 45,
        6, 173, 21, 166, 106, 66, 87, 243, 240, 81, 216, 78, 138, 14, 47, 145, 186, 128, 112, 71,
    ];

    /// Unpadded Base64-encoded example
    const UNPADDED_BASE64: &str =
        "AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti";
    const UNPADDED_BIN: &[u8] = &[
        0, 0, 0, 11, 115, 115, 104, 45, 101, 100, 50, 53, 53, 49, 57, 0, 0, 0, 32, 179, 62, 174,
        243, 126, 162, 223, 124, 170, 1, 13, 239, 222, 163, 78, 36, 31, 101, 241, 181, 41, 164,
        244, 62, 209, 67, 39, 245, 197, 74, 171, 98,
    ];

    #[test]
    fn decode_padded() {
        for chunk_size in 1..PADDED_BIN.len() {
            let mut decoder = Decoder::<Base64>::new(PADDED_BASE64.as_bytes()).unwrap();
            let mut buffer = [0u8; 128];

            for chunk in PADDED_BIN.chunks(chunk_size) {
                assert!(!decoder.is_finished());
                let decoded = decoder.decode(&mut buffer[..chunk.len()]).unwrap();
                assert_eq!(chunk, decoded);
            }

            assert!(decoder.is_finished());
        }
    }

    #[test]
    fn decode_unpadded() {
        for chunk_size in 1..UNPADDED_BIN.len() {
            let mut decoder = Decoder::<Base64Unpadded>::new(UNPADDED_BASE64.as_bytes()).unwrap();
            let mut buffer = [0u8; 64];

            for chunk in UNPADDED_BIN.chunks(chunk_size) {
                assert!(!decoder.is_finished());
                let decoded = decoder.decode(&mut buffer[..chunk.len()]).unwrap();
                assert_eq!(chunk, decoded);
            }

            assert!(decoder.is_finished());
        }
    }
}
