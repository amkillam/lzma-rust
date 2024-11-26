use super::{
    decoder::LZMADecoder,
    lz::LZDecoder,
    range_dec::{RangeDecoder, RangeDecoderBuffer},
};
use crate::io::{error, read_exact_error_kind, ErrorKind, Read};
#[cfg(feature = "no_std")]
use embedded_io::Error;
pub const COMPRESSED_SIZE_MAX: u64 = 1 << 16;
use crate::range_dec::RangeSource;

/// Decompresses a raw LZMA2 stream (no XZ headers).
/// # Examples
/// ```
/// use std::io::Read;
/// use lzma_rust::LZMA2Reader;
/// use lzma_rust::LZMA2Options;
/// let compressed = [1, 0, 12, 72, 101, 108, 108, 111, 44, 32, 119, 111, 114, 108, 100, 33, 0];
/// let mut reader = LZMA2Reader::new(compressed.as_slice(), LZMA2Options::DICT_SIZE_DEFAULT, None);
/// let mut decompressed = Vec::new();
/// reader.read_to_end(&mut decompressed);
/// assert_eq!(&decompressed[..], b"Hello, world!");
///
/// ```
pub struct LZMA2Reader<R> {
    inner: R,
    lz: LZDecoder,
    rc: RangeDecoder<RangeDecoderBuffer>,
    lzma: Option<LZMADecoder>,
    uncompressed_size: usize,
    is_lzma_chunk: bool,
    need_dict_reset: bool,
    need_props: bool,
    end_reached: bool,
    #[cfg(feature = "no_std")]
    error: Option<ErrorKind>,
    #[cfg(not(feature = "no_std"))]
    error: Option<(ErrorKind, String)>,
}
#[inline]
pub fn get_memory_usage(dict_size: u64) -> u64 {
    40 + COMPRESSED_SIZE_MAX / 1024 + get_dict_size(dict_size) / 1024
}

#[inline]
fn get_dict_size(dict_size: u64) -> u64 {
    (dict_size + 15) & !15
}

impl<R> LZMA2Reader<R> {
    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}

impl<R: Read> LZMA2Reader<R> {
    /// Create a new LZMA2 reader.
    /// `inner` is the reader to read compressed data from.
    /// `dict_size` is the dictionary size in bytes.
    pub fn new(inner: R, dict_size: u64, preset_dict: Option<&[u8]>) -> Self {
        let has_preset = preset_dict.as_ref().map(|a| !a.is_empty()).unwrap_or(false);
        let lz = LZDecoder::new(get_dict_size(dict_size) as _, preset_dict);
        let rc = RangeDecoder::new_buffer(COMPRESSED_SIZE_MAX as _);
        Self {
            inner,
            lz,
            rc,
            lzma: None,
            uncompressed_size: 0,
            is_lzma_chunk: false,
            need_dict_reset: !has_preset,
            need_props: true,
            end_reached: false,
            error: None,
        }
    }

    pub fn read_u8(&mut self) -> crate::io::read_exact_result!(R, u8) {
        let mut buf = [0; 1];
        self.inner.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn read_u16_be(&mut self) -> crate::io::read_exact_result!(R, u16) {
        let mut buf = [0; 2];
        self.inner.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    fn decode_chunk_header(&mut self) -> crate::io::read_exact_result!(R, ()) {
        let control = self.inner.read_u8()?;
        if control == 0x00 {
            self.end_reached = true;
            return Ok(());
        }

        if control >= 0xE0 || control == 0x01 {
            self.need_props = true;
            self.need_dict_reset = false;
            self.lz.reset();
        } else if self.need_dict_reset {
            return error!(
                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                "Corrupted input data (LZMA2:0)"
            );
        }
        if control >= 0x80 {
            self.is_lzma_chunk = true;
            self.uncompressed_size = ((control & 0x1F) as usize) << 16;
            self.uncompressed_size += self.read_u16_be()? as usize + 1;
            let compressed_size = self.read_u16_be()? as usize + 1;
            if control >= 0xC0 {
                self.need_props = false;
                self.decode_props()?;
            } else if self.need_props {
                return error!(
                    read_exact_error_kind!(R, ErrorKind::InvalidInput),
                    "Corrupted input data (LZMA2:1)"
                );
            } else if control >= 0xA0 {
                if let Some(l) = self.lzma.as_mut() {
                    l.reset()
                }
            }
            self.rc.prepare(&mut self.inner, compressed_size)?;
        } else if control > 0x02 {
            return error!(
                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                "Corrupted input data (LZMA2:2)"
            );
        } else {
            self.is_lzma_chunk = false;
            self.uncompressed_size = (self.read_u16_be()? + 1) as _;
        }
        Ok(())
    }

    fn decode_props(&mut self) -> crate::io::read_exact_result!(R, ()) {
        let props = self.inner.read_u8()?;
        if props > (4 * 5 + 4) * 9 + 8 {
            return error!(
                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                "Corrupted input data (LZMA2:3)"
            );
        }
        let pb = props / (9 * 5);
        let props = props - pb * 9 * 5;
        let lp = props / 9;
        let lc = props - lp * 9;
        if lc + lp > 4 {
            return error!(
                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                "Corrupted input data (LZMA2:4)"
            );
        }
        self.lzma = Some(LZMADecoder::new(lc as _, lp as _, pb as _));

        Ok(())
    }

    fn read_decode(&mut self, buf: &mut [u8]) -> crate::io::read_exact_result!(R, usize) {
        if buf.is_empty() {
            return Ok(0);
        }
        if let Some(e) = &self.error {
            #[cfg(feature = "no_std")]
            return Err(read_exact_error_kind!(R, *e));
            #[cfg(not(feature = "no_std"))]
            return error!(e.0, e.1.clone());
        }

        if self.end_reached {
            return Ok(0);
        }
        let mut size = 0;
        let mut len = buf.len();
        let mut off = 0;
        while len > 0 {
            if self.uncompressed_size == 0 {
                self.decode_chunk_header()?;
                if self.end_reached {
                    return Ok(size);
                }
            }

            let copy_size_max = self.uncompressed_size.min(len);
            if !self.is_lzma_chunk {
                self.lz.copy_uncompressed(&mut self.inner, copy_size_max)?;
            } else {
                self.lz.set_limit(copy_size_max);
                if let Some(lzma) = self.lzma.as_mut() {
                    match lzma.decode(&mut self.lz, &mut self.rc) {
                        Ok(_) => {}
                        Err(e) => {
                            #[cfg(not(feature = "no_std"))]
                            return error!(
                                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                                e.to_string()
                            );
                            #[cfg(feature = "no_std")]
                            {
                                let _ = e;
                                return error!(
                                    read_exact_error_kind!(R, ErrorKind::InvalidInput),
                                    ""
                                );
                            }
                        }
                    }
                }
            }

            {
                let copied_size = self.lz.flush(buf, off);
                off += copied_size;
                len -= copied_size;
                size += copied_size;
                self.uncompressed_size -= copied_size;
                if self.uncompressed_size == 0 && (!self.rc.is_finished() || self.lz.has_pending())
                {
                    return error!(
                        read_exact_error_kind!(R, ErrorKind::InvalidInput),
                        "rc not finished or lz has pending"
                    );
                }
            }
        }
        Ok(size)
    }
}

#[cfg(feature = "no_std")]
impl<R: Read> embedded_io::ErrorType for LZMA2Reader<R> {
    type Error = <R as embedded_io::ErrorType>::Error;
}

impl<R: Read> Read for LZMA2Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> crate::io::lzma_reader_result!(R, usize) {
        match self.read_decode(buf) {
            Ok(size) => Ok(size),
            Err(e) => {
                #[cfg(not(feature = "no_std"))]
                {
                    let error = e;
                    self.error = Some((error.kind(), error.to_string().to_string()));
                    error!(error.kind(), error.to_string())
                }
                #[cfg(feature = "no_std")]
                {
                    let error = match e {
                        embedded_io::ReadExactError::UnexpectedEof => ErrorKind::InvalidData,
                        embedded_io::ReadExactError::Other(e) => e.kind(),
                    };
                    self.error = Some(error);
                    Err(unsafe {
                        core::intrinsics::transmute_unchecked::<
                            embedded_io::ErrorKind,
                            <R as embedded_io::ErrorType>::Error,
                        >(error)
                    })
                }
            }
        }
    }
}
