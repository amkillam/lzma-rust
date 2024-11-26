use crate::io::{error, write_error_kind, ErrorKind, Write};

use super::{range_enc::RangeEncoder, CountingWriter, LZMA2Options};

use super::encoder::{LZMAEncoder, LZMAEncoderModes};

/// Compresses into the legacy .lzma file format or into a raw LZMA stream
///
/// # Examples
/// ```
/// use std::io::Write;
/// use lzma_rust::{LZMA2Options, LZMAWriter};
/// use lzma_rust::CountingWriter;
/// let s = b"Hello, world!";
/// let mut out = Vec::new();
/// let mut options = LZMA2Options::with_preset(6);
/// options.dict_size = LZMA2Options::DICT_SIZE_DEFAULT;
///
/// let mut w = LZMAWriter::new_no_header(CountingWriter::new(&mut out), &options, false).unwrap();
/// w.write_all(s).unwrap();
/// w.write(&[]).unwrap();
///
/// ```
///
pub struct LZMAWriter<W: Write> {
    pub rc: RangeEncoder<CountingWriter<W>>,
    lzma: LZMAEncoder,
    use_end_marker: bool,
    finished: bool,
    current_uncompressed_size: u64,
    expected_uncompressed_size: Option<u64>,
    props: u8,
    mode: LZMAEncoderModes,
}

impl<W: Write> LZMAWriter<W> {
    pub fn new(
        mut out: CountingWriter<W>,
        options: &LZMA2Options,
        use_header: bool,
        use_end_marker: bool,
        expected_uncompressed_size: Option<u64>,
    ) -> crate::io::write_result!(CountingWriter<W>, LZMAWriter<W>) {
        let (mut lzma, mode) = LZMAEncoder::new(
            options.mode,
            options.lc,
            options.lp,
            options.pb,
            options.mf,
            options.depth_limit,
            options.dict_size,
            options.nice_len as usize,
        );
        if let Some(preset_dict) = &options.preset_dict {
            if use_header {
                return error!(
                    write_error_kind!(CountingWriter<W>, ErrorKind::Unsupported),
                    "Header is not supported with preset dict"
                );
            }
            lzma.lz.set_preset_dict(options.dict_size, preset_dict);
        }

        let props = options.get_props();
        if use_header {
            out.write_all(&[props])?;
            let mut dict_size = options.dict_size;
            for _i in 0..4 {
                out.write_all(&[(dict_size & 0xFF) as u8])?;
                dict_size >>= 8;
            }
            let expected_compressed_size = expected_uncompressed_size.unwrap_or(u64::MAX);
            for i in 0..8 {
                out.write_all(&[((expected_compressed_size >> (i * 8)) & 0xFF) as u8])?;
            }
        }

        let rc = RangeEncoder::new(out);
        Ok(LZMAWriter {
            rc,
            lzma,
            use_end_marker,
            finished: false,
            current_uncompressed_size: 0,
            expected_uncompressed_size,
            props,
            mode,
        })
    }

    #[inline]
    pub fn new_use_header(
        out: CountingWriter<W>,
        options: &LZMA2Options,
        input_size: Option<u64>,
    ) -> crate::io::write_result!(CountingWriter<W>, Self) {
        Self::new(out, options, true, input_size.is_none(), input_size)
    }

    #[inline]
    pub fn new_no_header(
        out: CountingWriter<W>,
        options: &LZMA2Options,
        use_end_marker: bool,
    ) -> crate::io::write_result!(CountingWriter<W>, Self) {
        Self::new(out, options, false, use_end_marker, None)
    }

    #[inline]
    pub fn props(&self) -> u8 {
        self.props
    }

    #[inline]
    pub fn get_uncompressed_size(&self) -> u64 {
        self.current_uncompressed_size
    }

    pub fn finish(&mut self) -> crate::io::write_result!(CountingWriter<W>, ()) {
        if !self.finished {
            if let Some(exp) = self.expected_uncompressed_size {
                if exp != self.current_uncompressed_size {
                    return error!(
                        write_error_kind!(CountingWriter<W>, ErrorKind::InvalidInput),
                        "Expected compressed size does not match actual compressed size"
                    );
                }
            }
            self.lzma.lz.set_finishing();
            self.lzma.encode_for_lzma1(&mut self.rc, &mut self.mode)?;
            if self.use_end_marker {
                self.lzma.encode_lzma1_end_marker(&mut self.rc)?;
            }
            self.rc.finish()?;
            self.finished = true;
        }
        Ok(())
    }
}

#[cfg(feature = "no_std")]
impl<W: Write> embedded_io::ErrorType for LZMAWriter<W> {
    type Error = <W as embedded_io::ErrorType>::Error;
}

impl<W: Write> Write for LZMAWriter<W> {
    fn write(&mut self, buf: &[u8]) -> crate::io::write_result!(W, usize) {
        if self.finished {
            return error!(
                write_error_kind!(W, ErrorKind::InvalidInput),
                "Already finished"
            );
        }
        if buf.is_empty() {
            self.finish()?;
            self.rc.inner().write_all(buf)?;
            return Ok(0);
        }
        if let Some(exp) = self.expected_uncompressed_size {
            if exp < self.current_uncompressed_size + buf.len() as u64 {
                return error!(
                    write_error_kind!(W, ErrorKind::InvalidInput),
                    "Expected compressed size does not match actual compressed size"
                );
            }
        }
        self.current_uncompressed_size += buf.len() as u64;
        let mut len = buf.len();
        let mut off = 0;
        while len > 0 {
            let used = self.lzma.lz.fill_window(&buf[off..]);
            off += used;
            len -= used;
            self.lzma.encode_for_lzma1(&mut self.rc, &mut self.mode)?;
        }

        Ok(off)
    }

    fn flush(&mut self) -> crate::io::write_result!(W, ()) {
        Ok(())
    }
}
