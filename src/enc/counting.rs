#[cfg(feature = "no_std")]
use crate::alloc::rc::Rc;
use crate::io::Write;
use core::cell::Cell;
#[cfg(not(feature = "no_std"))]
use std::rc::Rc;

pub struct CountingWriter<W> {
    pub inner: W,
    counting: Rc<Cell<usize>>,
    written_bytes: usize,
}

#[cfg(feature = "no_std")]
impl<W: Write> embedded_io::ErrorType for CountingWriter<W> {
    type Error = <W as embedded_io::ErrorType>::Error;
}

impl<W: Write> CountingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            counting: Rc::new(Cell::new(0)),
            written_bytes: 0,
        }
    }
    pub fn written_bytes(&self) -> usize {
        self.written_bytes
    }

    pub fn counting(&self) -> Rc<Cell<usize>> {
        Rc::clone(&self.counting)
    }

    pub fn inner(&self) -> &W {
        &self.inner
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> crate::io::write_result!(W, usize) {
        let len = self.inner.write(buf)?;
        self.written_bytes += len;
        self.counting.set(self.written_bytes);
        Ok(len)
    }

    fn flush(&mut self) -> crate::io::write_result!(W, ()) {
        self.inner.flush()
    }
}
