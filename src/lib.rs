#![cfg_attr(feature = "no_std", no_std)]
#![cfg_attr(
    all(feature = "no_std", feature = "alloc"),
    feature(stmt_expr_attributes),
    feature(core_intrinsics),
    allow(internal_features)
)]
#![cfg_attr(
    not(feature = "alloc"),
    feature(generic_const_exprs),
    feature(const_trait_impl),
    feature(const_intrinsic_copy),
    feature(const_for),
    feature(const_slice_from_raw_parts_mut),
    allow(incomplete_features)
)]

#[cfg_attr(feature = "alloc", path = "./decoder_alloc.rs")]
#[cfg_attr(not(feature = "alloc"), path = "./decoder_no_alloc.rs")]
pub mod decoder;
pub mod lz;
#[cfg_attr(feature = "alloc", path = "./lzma2_reader_alloc.rs")]
#[cfg_attr(not(feature = "alloc"), path = "./lzma2_reader_no_alloc.rs")]
pub mod lzma2_reader;
#[cfg_attr(feature = "alloc", path = "./lzma_reader_alloc.rs")]
#[cfg_attr(not(feature = "alloc"), path = "./lzma_reader_no_alloc.rs")]
pub mod lzma_reader;
#[cfg_attr(feature = "alloc", path = "./range_dec_alloc.rs")]
#[cfg_attr(not(feature = "alloc"), path = "./range_dec_no_alloc.rs")]
mod range_dec;
mod state;

#[cfg(all(feature = "no_std", feature = "alloc"))]
#[macro_use]
pub extern crate alloc;

pub use lzma2_reader::get_memory_usage as lzma2_get_memory_usage;
pub use lzma2_reader::LZMA2Reader;
pub use lzma_reader::get_memory_usage as lzma_get_memory_usage;
pub use lzma_reader::get_memory_usage_by_props as lzma_get_memory_usage_by_props;
pub use lzma_reader::LZMAReader;
#[cfg(all(feature = "encoder", feature = "alloc"))]
pub mod enc;
#[cfg(all(feature = "encoder", feature = "alloc"))]
pub use enc::*;

use state::*;

#[cfg(all(not(feature = "no_std"), feature = "alloc"))]
mod io_alloc {
    macro_rules! error {
        ($kind:expr, $msg:expr) => {
            Err(std::io::Error::new($kind, $msg))
        };
    }

    pub(crate) use error;

    macro_rules! read_exact_result {
        ($reader: ty, $out: ty) => {
            std::io::Result<$out>
        };
    }

    macro_rules! write_result {
        ($writer: ty, $out: ty) => {
            std::io::Result<$out>
        };
    }

    macro_rules! lzma_reader_result {
        ($reader: ty, $out: ty) => {
            std::io::Result<$out>
        };
    }

    pub(crate) use lzma_reader_result;
    pub(crate) use read_exact_result;
    pub(crate) use write_result;

    macro_rules! read_exact_error_kind {
        ($reader: ty, $kind:expr) => {
            $kind
        };
    }

    macro_rules! write_error_kind {
        ($writer: ty, $kind:expr) => {
            $kind
        };
    }

    pub(crate) use read_exact_error_kind;
    pub(crate) use write_error_kind;

    macro_rules! transmute_result_error_type {
        ($err: expr, $out: ty, $src: ty, $dst: ty) => {
            $err
        };
    }
    pub(crate) use transmute_result_error_type;
}

#[cfg(not(feature = "no_std"))]
pub mod io {
    pub use std::io::*;

    #[cfg(feature = "alloc")]
    pub(crate) use super::io_alloc::*;

    pub type Result<T> = std::io::Result<T>;
}
#[cfg(not(feature = "no_std"))]
pub use std::{vec, vec::Vec};

#[cfg(all(feature = "no_std", feature = "alloc"))]
pub use alloc::{vec, vec::Vec};

#[cfg(all(feature = "no_std", feature = "alloc"))]
mod io_alloc {
    macro_rules! read_exact_error_kind {
        ($reader: ty, $kind:expr) => {{
            let kind = unsafe {
                core::intrinsics::transmute_unchecked::<
                    embedded_io::ErrorKind,
                    <$reader as embedded_io::ErrorType>::Error,
                >($kind)
            };
            embedded_io::ReadExactError::<<$reader as embedded_io::ErrorType>::Error>::Other(kind)
        }};
    }
    pub(crate) use read_exact_error_kind;

    macro_rules! lzma_reader_result {
        ($reader: ty, $out: ty) => {
            core::result::Result<$out, <$reader as embedded_io::ErrorType>::Error>
        };
    }

    pub(crate) use lzma_reader_result;

    macro_rules! write_error_kind {
        ($writer: ty, $kind:expr) => {{
            let kind = unsafe {
                core::intrinsics::transmute_unchecked::<
                    embedded_io::ErrorKind,
                    <$writer as embedded_io::ErrorType>::Error,
                >($kind)
            };
            kind
        }};
    }
    macro_rules! error {
        ($kind:expr, $msg: expr) => {
            Err($kind)
        };
    }

    pub(crate) use error;
    pub(crate) use write_error_kind;

    macro_rules! transmute_result_error_type {
        ($err: expr, $out: ty, $src: ty, $dst: ty) => {
            unsafe {
                core::intrinsics::transmute_unchecked::<
                    core::result::Result<$out, <$src as embedded_io::ErrorType>::Error>,
                    core::result::Result<$out, <$dst as embedded_io::ErrorType>::Error>,
                >($err)
            }
        };
    }

    pub(crate) use transmute_result_error_type;

    macro_rules! read_exact_result {
        ($reader: ty, $out: ty) => {
            core::result::Result<$out, embedded_io::ReadExactError<<$reader as embedded_io::ErrorType>::Error>>
        };
    }
    pub(crate) use read_exact_result;

    macro_rules! write_result {
        ($writer: ty, $out: ty) => {
            core::result::Result<$out, <$writer as embedded_io::ErrorType>::Error>
        };
    }
    pub(crate) use write_result;
}

#[cfg(feature = "no_std")]
pub mod io {

    pub type Result<T> = core::result::Result<T, embedded_io::ErrorKind>;
    pub use embedded_io::*;

    #[cfg(feature = "alloc")]
    pub(crate) use super::io_alloc::*;
}

pub const DICT_SIZE_MIN: u64 = 4096;
pub const DICT_SIZE_MAX: u64 = u64::MAX & !15_u64;

const LOW_SYMBOLS: usize = 1 << 3;
const MID_SYMBOLS: usize = 1 << 3;
const HIGH_SYMBOLS: usize = 1 << 8;

const POS_STATES_MAX: usize = 1 << 4;
const MATCH_LEN_MIN: usize = 2;
#[cfg(feature = "alloc")]
const MATCH_LEN_MAX: usize = MATCH_LEN_MIN + LOW_SYMBOLS + MID_SYMBOLS + HIGH_SYMBOLS - 1;

const DIST_STATES: usize = 4;
const DIST_SLOTS: usize = 1 << 6;
const DIST_MODEL_START: usize = 4;
const DIST_MODEL_END: usize = 14;
#[cfg(feature = "alloc")]
const FULL_DISTANCES: usize = 1 << (DIST_MODEL_END / 2);

const ALIGN_BITS: usize = 4;
const ALIGN_SIZE: usize = 1 << ALIGN_BITS;
#[cfg(feature = "alloc")]
const ALIGN_MASK: usize = ALIGN_SIZE - 1;

const REPS: usize = 4;

const SHIFT_BITS: u64 = 8;
#[cfg(feature = "alloc")]
const TOP_MASK: u64 = 0xFF000000;
const BIT_MODEL_TOTAL_BITS: u64 = 11;
const BIT_MODEL_TOTAL: u64 = 1 << BIT_MODEL_TOTAL_BITS;
const PROB_INIT: u16 = (BIT_MODEL_TOTAL / 2) as u16;
const MOVE_BITS: u64 = 5;
const DIST_SPECIAL_INDEX: [usize; 10] = [0, 2, 4, 8, 12, 20, 28, 44, 60, 92];
const DIST_SPECIAL_END: [usize; 10] = [2, 4, 8, 12, 20, 28, 44, 60, 92, 124];

#[cfg(feature = "alloc")]
pub struct LZMACoder {
    pub(crate) pos_mask: u64,
    pub(crate) reps: [i64; REPS],
    pub(crate) state: State,
    pub(crate) is_match: [[u16; POS_STATES_MAX]; state::STATES],
    pub(crate) is_rep: [u16; state::STATES],
    pub(crate) is_rep0: [u16; state::STATES],
    pub(crate) is_rep1: [u16; state::STATES],
    pub(crate) is_rep2: [u16; state::STATES],
    pub(crate) is_rep0_long: [[u16; POS_STATES_MAX]; state::STATES],
    pub(crate) dist_slots: [[u16; DIST_SLOTS]; DIST_STATES],
    dist_special: [u16; 124],
    dist_align: [u16; ALIGN_SIZE],
}

/// SAFETY: std version of this function ensures that dst and src are the same length
/// and that they do not overlap. This function does not check for that, due to inability to panic!
/// in const fn. It is up to the caller to ensure that the conditions are met.
/// However, a compile-time error should be raised if the conditions are not met, as the fn is
/// const.
#[cfg(not(feature = "alloc"))]
pub const fn copy_from_slice<T: Copy>(dst: &mut [T], src: &[T]) {
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), src.len());
    }
}

#[cfg(not(feature = "alloc"))]
pub struct LZMACoder<const PB: u64> {
    pub(crate) reps: [i64; REPS],
    pub(crate) state: State,
    pub(crate) is_match: [[u16; POS_STATES_MAX]; state::STATES],
    pub(crate) is_rep: [u16; state::STATES],
    pub(crate) is_rep0: [u16; state::STATES],
    pub(crate) is_rep1: [u16; state::STATES],
    pub(crate) is_rep2: [u16; state::STATES],
    pub(crate) is_rep0_long: [[u16; POS_STATES_MAX]; state::STATES],
    pub(crate) dist_slots: [[u16; DIST_SLOTS]; DIST_STATES],
    dist_special: [u16; 124],
    dist_align: [u16; ALIGN_SIZE],
}

pub(crate) const fn coder_get_dict_size(len: usize) -> usize {
    if len < DIST_STATES + MATCH_LEN_MIN {
        len - MATCH_LEN_MIN
    } else {
        DIST_STATES - 1
    }
}
#[cfg(feature = "alloc")]
pub(crate) const fn get_dist_state(len: u64) -> u64 {
    (if (len as usize) < DIST_STATES + MATCH_LEN_MIN {
        len as usize - MATCH_LEN_MIN
    } else {
        DIST_STATES - 1
    }) as u64
}

#[cfg(feature = "alloc")]
impl LZMACoder {
    pub fn new(pb: usize) -> Self {
        let mut c = Self {
            pos_mask: (1 << pb) - 1,
            reps: Default::default(),
            state: Default::default(),
            is_match: Default::default(),
            is_rep: Default::default(),
            is_rep0: Default::default(),
            is_rep1: Default::default(),
            is_rep2: Default::default(),
            is_rep0_long: Default::default(),
            dist_slots: [[Default::default(); DIST_SLOTS]; DIST_STATES],
            dist_special: [Default::default(); 124],
            dist_align: Default::default(),
        };
        c.reset();
        c
    }

    pub fn reset(&mut self) {
        self.reps = [0; REPS];
        self.state.reset();
        for ele in self.is_match.iter_mut() {
            init_probs(ele);
        }
        init_probs(&mut self.is_rep);
        init_probs(&mut self.is_rep0);
        init_probs(&mut self.is_rep1);
        init_probs(&mut self.is_rep2);

        for ele in self.is_rep0_long.iter_mut() {
            init_probs(ele);
        }
        for ele in self.dist_slots.iter_mut() {
            init_probs(ele);
        }
        init_probs(&mut self.dist_special);
        init_probs(&mut self.dist_align);
    }

    #[inline(always)]
    pub fn get_dist_special(&mut self, i: usize) -> &mut [u16] {
        &mut self.dist_special[DIST_SPECIAL_INDEX[i]..DIST_SPECIAL_END[i]]
    }
}

#[cfg(not(feature = "alloc"))]
impl<const PB: u64> LZMACoder<PB> {
    const POS_MASK: u64 = (1 << PB) - 1;
    pub const fn new() -> Self {
        let mut c = Self {
            reps: [0i64; REPS],
            state: State::new(),
            is_match: [[0u16; POS_STATES_MAX]; state::STATES],
            is_rep: [0u16; state::STATES],
            is_rep0: [0u16; state::STATES],
            is_rep1: [0u16; state::STATES],
            is_rep2: [0u16; state::STATES],
            is_rep0_long: [[0u16; POS_STATES_MAX]; state::STATES],
            dist_slots: [[0u16; DIST_SLOTS]; DIST_STATES],
            dist_special: [0u16; 124],
            dist_align: [0u16; ALIGN_SIZE],
        };
        c.reset();
        c
    }

    pub const fn reset(&mut self) {
        self.reps = [0; REPS];
        self.state.reset();
        self.is_match = [[PROB_INIT; POS_STATES_MAX]; state::STATES];

        init_probs(&mut self.is_rep);
        init_probs(&mut self.is_rep0);
        init_probs(&mut self.is_rep1);
        init_probs(&mut self.is_rep2);

        self.is_rep0_long = [[PROB_INIT; POS_STATES_MAX]; state::STATES];
        self.dist_slots = [[PROB_INIT; DIST_SLOTS]; DIST_STATES];
        self.dist_special = [PROB_INIT; 124];
        self.dist_align = [PROB_INIT; ALIGN_SIZE];
    }

    #[inline(always)]
    pub const fn get_dist_special(&mut self, i: usize) -> &mut [u16] {
        let start = DIST_SPECIAL_INDEX[i];
        let end = DIST_SPECIAL_END[i];
        let len = end - start;

        unsafe {
            let ptr = self.dist_special.as_mut_ptr().add(start);
            core::slice::from_raw_parts_mut(ptr, len)
        }
    }
}

#[inline(always)]
pub(crate) const fn init_probs<const N: usize>(probs: &mut [u16; N]) {
    *probs = [PROB_INIT; N];
}

#[cfg(feature = "alloc")]
pub(crate) struct LiteralCoder {
    lc: u64,
    literal_pos_mask: u64,
}

#[cfg(not(feature = "alloc"))]
pub(crate) struct LiteralCoder<const LC: u64, const LP: u64>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiteralSubcoder {
    probs: [u16; 0x300],
}

impl LiteralSubcoder {
    pub const fn new() -> Self {
        let probs = [PROB_INIT; 0x300];
        Self { probs }
    }

    #[cfg(feature = "alloc")]
    pub const fn reset(&mut self) {
        self.probs = [PROB_INIT; 0x300];
    }
}

#[cfg(feature = "alloc")]
impl LiteralCoder {
    pub fn new(lc: u64, lp: u64) -> Self {
        Self {
            lc,
            literal_pos_mask: (1 << lp) - 1,
        }
    }
    pub(crate) fn get_sub_coder_index(&self, prev_byte: u64, pos: u64) -> u64 {
        let low = prev_byte >> (8 - self.lc);
        let high = (pos & self.literal_pos_mask) << self.lc;
        low + high
    }
}

#[cfg(not(feature = "alloc"))]
impl<const LC: u64, const LP: u64> LiteralCoder<LC, LP> {
    const LITERAL_POS_MASK: u64 = (1 << LP) - 1;
    pub const fn new() -> Self {
        Self
    }
    pub(crate) fn get_sub_coder_index(&self, prev_byte: u64, pos: u64) -> u64 {
        let low = prev_byte >> (8 - LC);
        let high = (pos & Self::LITERAL_POS_MASK) << LC;
        low + high
    }
}

pub(crate) struct LengthCoder {
    choice: [u16; 2],
    low: [[u16; LOW_SYMBOLS]; POS_STATES_MAX],
    mid: [[u16; MID_SYMBOLS]; POS_STATES_MAX],
    high: [u16; HIGH_SYMBOLS],
}

impl LengthCoder {
    pub const fn new() -> Self {
        Self {
            choice: [0u16; 2],
            low: [[0u16; LOW_SYMBOLS]; POS_STATES_MAX],
            mid: [[0u16; MID_SYMBOLS]; POS_STATES_MAX],
            high: [0u16; HIGH_SYMBOLS],
        }
    }

    pub const fn reset(&mut self) {
        self.choice = [PROB_INIT; 2];
        self.low = [[PROB_INIT; LOW_SYMBOLS]; POS_STATES_MAX];
        self.mid = [[PROB_INIT; MID_SYMBOLS]; POS_STATES_MAX];
        self.high = [PROB_INIT; HIGH_SYMBOLS];
    }
}
