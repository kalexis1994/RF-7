//! DEFLATE, as RFC 1951 defines it.
//!
//! A cartridge that arrives inside a ZIP arrives compressed, and this crate
//! has no dependencies and no allocator — so the decompressor is written out
//! here, and it decompresses into a buffer the caller owns. That buffer is
//! also the window: every back-reference in a DEFLATE stream points at
//! output already produced, so nothing else has to be kept.
//!
//! The decoder is the straightforward canonical-Huffman one, a bit at a
//! time. A cartridge is a few kilobytes and this runs on the control thread
//! once, when a file is installed, so the table-driven speed-ups that make
//! a general-purpose library complicated would buy nothing and cost the
//! ability to read this and be sure of it.
//!
//! Every length in the stream is checked against the buffer before it is
//! written, every code against the table that must hold it, and every
//! distance against how much output exists — so no input, however malformed
//! or hostile, can do more than return an error.

/// Bits in the longest Huffman code the format allows.
const MAX_BITS: usize = 15;
/// Literal/length alphabet: 0..=255 literals, 256 end of block, 257..=285
/// lengths, and the two the format leaves undefined.
const MAX_SYMBOLS: usize = 288;
/// The alphabet that describes a dynamic block's code lengths.
const CODE_LENGTH_SYMBOLS: usize = 19;
/// The order those nineteen lengths are written in.
const CODE_LENGTH_ORDER: [usize; CODE_LENGTH_SYMBOLS] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
/// Match lengths for symbols 257..=285, and the extra bits each one reads.
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// Match distances for the thirty distance symbols, and their extra bits.
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Why a stream could not be read. Every one of them is a refusal, never a
/// partial result: a cartridge half decompressed is not a cartridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InflateError {
    /// The stream ended in the middle of something.
    Truncated,
    /// A block type RFC 1951 leaves undefined.
    Block,
    /// A stored block whose length and its complement disagree.
    Stored,
    /// A Huffman table that cannot exist, or a code no table holds.
    Code,
    /// A back-reference reaching further back than the output goes.
    Distance,
    /// More output than the caller made room for.
    Output,
}

impl core::fmt::Display for InflateError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let text = match self {
            Self::Truncated => "the compressed stream ends in the middle",
            Self::Block => "a compressed block of a kind DEFLATE does not define",
            Self::Stored => "an uncompressed block whose length contradicts itself",
            Self::Code => "a Huffman code the stream's own tables do not hold",
            Self::Distance => "a back-reference pointing before the start of the data",
            Self::Output => "more data than the buffer it was being read into",
        };
        formatter.write_str(text)
    }
}

impl core::error::Error for InflateError {}

/// Decompress `input` into `output`, returning how many bytes it produced.
///
/// The output buffer is the only limit on how much a stream may expand, so a
/// caller bounds a compression bomb by sizing it.
pub fn inflate(input: &[u8], output: &mut [u8]) -> Result<usize, InflateError> {
    let mut bits = Bits::new(input);
    let mut written = 0;
    loop {
        let last = bits.take(1)? == 1;
        written = match bits.take(2)? {
            0 => stored(&mut bits, output, written)?,
            1 => {
                let (literals, distances) = fixed_tables()?;
                compressed(&mut bits, &literals, &distances, output, written)?
            }
            2 => {
                let (literals, distances) = dynamic_tables(&mut bits)?;
                compressed(&mut bits, &literals, &distances, output, written)?
            }
            _ => return Err(InflateError::Block),
        };
        if last {
            return Ok(written);
        }
    }
}

/// The stream, read a bit at a time, least significant bit first.
struct Bits<'a> {
    data: &'a [u8],
    /// How many bits have been consumed, so the byte and the bit within it
    /// are one number rather than two that can disagree.
    position: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// The next `count` bits as an integer. Zero bits is zero, and consumes
    /// nothing — which is what a symbol with no extra bits asks for.
    fn take(&mut self, count: u32) -> Result<u32, InflateError> {
        let mut value = 0;
        for index in 0..count {
            let byte = self
                .data
                .get(self.position >> 3)
                .ok_or(InflateError::Truncated)?;
            value |= u32::from((byte >> (self.position & 7)) & 1) << index;
            self.position += 1;
        }
        Ok(value)
    }

    /// Skip to the next byte boundary, as a stored block's header requires.
    fn align(&mut self) {
        self.position = self.position.next_multiple_of(8);
    }

    /// The next `count` whole bytes, from a position already aligned.
    fn bytes(&mut self, count: usize) -> Result<&'a [u8], InflateError> {
        let start = self.position >> 3;
        let end = start.checked_add(count).ok_or(InflateError::Truncated)?;
        let slice = self.data.get(start..end).ok_or(InflateError::Truncated)?;
        self.position = end << 3;
        Ok(slice)
    }
}

/// One canonical Huffman table: how many codes there are of each length, and
/// the symbols in the order the lengths put them.
struct Huffman {
    counts: [u16; MAX_BITS + 1],
    symbols: [u16; MAX_SYMBOLS],
}

impl Huffman {
    /// Build the table one code length per symbol, a length of zero meaning
    /// the symbol does not appear.
    ///
    /// A table that over-subscribes its lengths cannot exist and is refused.
    /// One that under-subscribes them is refused as well, except when it
    /// holds a single code or none at all: a block whose matches all share
    /// one distance writes exactly that, and a block with no matches writes
    /// an empty distance table.
    fn new(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0; MAX_BITS + 1];
        for &length in lengths {
            let length = usize::from(length);
            if length > MAX_BITS {
                return Err(InflateError::Code);
            }
            counts[length] += 1;
        }
        let coded = lengths.len() - usize::from(counts[0]);
        // A code of each length halves what is left for the longer ones.
        let mut left = 1_i32;
        for &count in &counts[1..=MAX_BITS] {
            left <<= 1;
            left -= i32::from(count);
            if left < 0 {
                return Err(InflateError::Code);
            }
        }
        if left > 0 && coded > 1 {
            return Err(InflateError::Code);
        }
        let mut offsets = [0_u16; MAX_BITS + 2];
        for length in 1..=MAX_BITS {
            offsets[length + 1] = offsets[length] + counts[length];
        }
        let mut symbols = [0; MAX_SYMBOLS];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                let slot = &mut offsets[usize::from(length)];
                symbols[usize::from(*slot)] = symbol as u16;
                *slot += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    /// Read one symbol: walk the code lengths, widening the code by a bit at
    /// a time until it falls inside the codes of that length.
    fn decode(&self, bits: &mut Bits<'_>) -> Result<u16, InflateError> {
        let mut code = 0_i32;
        let mut first = 0_i32;
        let mut index = 0_i32;
        for length in 1..=MAX_BITS {
            code |= bits.take(1)? as i32;
            let count = i32::from(self.counts[length]);
            if code - first < count {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(InflateError::Code)
    }
}

/// A block that was not worth compressing: a length, its complement, and the
/// bytes themselves.
fn stored(bits: &mut Bits<'_>, output: &mut [u8], written: usize) -> Result<usize, InflateError> {
    bits.align();
    let header = bits.bytes(4)?;
    let length = u16::from_le_bytes([header[0], header[1]]);
    let complement = u16::from_le_bytes([header[2], header[3]]);
    if length != !complement {
        return Err(InflateError::Stored);
    }
    let data = bits.bytes(usize::from(length))?;
    let end = written
        .checked_add(data.len())
        .ok_or(InflateError::Output)?;
    output
        .get_mut(written..end)
        .ok_or(InflateError::Output)?
        .copy_from_slice(data);
    Ok(end)
}

/// The literal/length and distance tables every fixed block shares.
///
/// The distance table is built with all thirty-two five-bit codes, which is
/// what makes it complete; the two the format never assigns are refused
/// where they would be used, not where they are counted.
fn fixed_tables() -> Result<(Huffman, Huffman), InflateError> {
    let mut lengths = [0_u8; MAX_SYMBOLS];
    for (symbol, length) in lengths.iter_mut().enumerate() {
        *length = match symbol {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    Ok((Huffman::new(&lengths)?, Huffman::new(&[5; 32])?))
}

/// The tables a dynamic block carries in front of itself, themselves
/// Huffman-coded.
fn dynamic_tables(bits: &mut Bits<'_>) -> Result<(Huffman, Huffman), InflateError> {
    let literal_count = bits.take(5)? as usize + 257;
    let distance_count = bits.take(5)? as usize + 1;
    let code_count = bits.take(4)? as usize + 4;
    if literal_count > 286 || distance_count > 30 {
        return Err(InflateError::Code);
    }
    let mut code_lengths = [0_u8; CODE_LENGTH_SYMBOLS];
    for &symbol in &CODE_LENGTH_ORDER[..code_count] {
        code_lengths[symbol] = bits.take(3)? as u8;
    }
    let table = Huffman::new(&code_lengths)?;

    // The two alphabets are written as one run of lengths and split after.
    let total = literal_count + distance_count;
    let mut lengths = [0_u8; MAX_SYMBOLS + 32];
    let mut index = 0;
    while index < total {
        let symbol = table.decode(bits)?;
        let (repeat, value) = match symbol {
            0..=15 => (1, symbol as u8),
            16 => {
                // Repeat the previous length, which there must be one of.
                let previous = *lengths
                    .get(index.wrapping_sub(1))
                    .ok_or(InflateError::Code)?;
                (3 + bits.take(2)? as usize, previous)
            }
            17 => (3 + bits.take(3)? as usize, 0),
            18 => (11 + bits.take(7)? as usize, 0),
            _ => return Err(InflateError::Code),
        };
        let end = index + repeat;
        if end > total {
            return Err(InflateError::Code);
        }
        lengths[index..end].fill(value);
        index = end;
    }
    let literals = Huffman::new(&lengths[..literal_count])?;
    let distances = Huffman::new(&lengths[literal_count..total])?;
    Ok((literals, distances))
}

/// A compressed block: literals as themselves, matches as a length and a
/// distance back into what has already been written.
fn compressed(
    bits: &mut Bits<'_>,
    literals: &Huffman,
    distances: &Huffman,
    output: &mut [u8],
    mut written: usize,
) -> Result<usize, InflateError> {
    loop {
        let symbol = usize::from(literals.decode(bits)?);
        if symbol < 256 {
            *output.get_mut(written).ok_or(InflateError::Output)? = symbol as u8;
            written += 1;
            continue;
        }
        if symbol == 256 {
            return Ok(written);
        }
        let index = symbol - 257;
        if index >= LENGTH_BASE.len() {
            return Err(InflateError::Code);
        }
        let length =
            usize::from(LENGTH_BASE[index]) + bits.take(u32::from(LENGTH_EXTRA[index]))? as usize;
        let symbol = usize::from(distances.decode(bits)?);
        if symbol >= DISTANCE_BASE.len() {
            return Err(InflateError::Code);
        }
        let distance = usize::from(DISTANCE_BASE[symbol])
            + bits.take(u32::from(DISTANCE_EXTRA[symbol]))? as usize;
        if distance > written {
            return Err(InflateError::Distance);
        }
        let end = written.checked_add(length).ok_or(InflateError::Output)?;
        if end > output.len() {
            return Err(InflateError::Output);
        }
        // Byte at a time, because a match may overlap what it is copying:
        // that is how a run of one byte is written.
        let mut from = written - distance;
        while written < end {
            output[written] = output[from];
            written += 1;
            from += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `RF-7 RF-7 RF-7 cartridge`, as a fixed-Huffman block: literals and a
    /// match reaching back over the repeated phrase.
    const DEFLATED_FIXED: [u8; 18] = [
        0x0b, 0x72, 0xd3, 0x35, 0x57, 0x08, 0x82, 0x13, 0xc9, 0x89, 0x45, 0x25, 0x45, 0x99, 0x29,
        0xe9, 0xa9, 0x00,
    ];
    /// Twelve copies of one line of voice names, which is enough repetition
    /// that an archiver builds tables of its own for it.
    const DEFLATED_DYNAMIC: [u8; 41] = [
        0x73, 0x0a, 0x72, 0x0c, 0x0e, 0x56, 0x30, 0x54, 0x08, 0x0e, 0x09, 0xf2, 0xf4, 0x73, 0x07,
        0xb1, 0x02, 0x3c, 0x1d, 0xfd, 0xfc, 0x81, 0xb4, 0xab, 0x1e, 0x8c, 0xe5, 0x1e, 0xea, 0x19,
        0xe2, 0x18, 0x04, 0x64, 0x38, 0x8d, 0x2a, 0x1e, 0x49, 0x8a, 0x01,
    ];
    /// Seven hundred of one byte: one literal and a match that overlaps
    /// itself, which is the only way a run is written.
    const DEFLATED_OVERLAP: [u8; 9] = [0xab, 0xaa, 0x1a, 0x05, 0xa3, 0x60, 0x68, 0x02, 0x00];

    const PHRASE: &[u8] = b"BRASS 1 STRINGS 1 PIANO 1 E.PIANO 1 GUITAR 1 ";

    /// A final stored block, which is its own header and then its bytes.
    fn stored_block(data: &[u8], stream: &mut [u8]) -> usize {
        let length = data.len() as u16;
        stream[0] = 0b001;
        stream[1..3].copy_from_slice(&length.to_le_bytes());
        stream[3..5].copy_from_slice(&(!length).to_le_bytes());
        stream[5..5 + data.len()].copy_from_slice(data);
        5 + data.len()
    }

    #[test]
    fn a_stored_block_is_copied_through() {
        let data = b"RF-7 VOICE CARTRIDGE";
        let mut stream = [0; 64];
        let length = stored_block(data, &mut stream);
        let mut output = [0; 64];
        assert_eq!(inflate(&stream[..length], &mut output), Ok(data.len()));
        assert_eq!(&output[..data.len()], data);
    }

    #[test]
    fn a_stored_block_that_contradicts_itself_is_refused() {
        let mut stream = [0; 32];
        let length = stored_block(b"abcd", &mut stream);
        stream[3] ^= 0xff;
        let mut output = [0; 32];
        assert_eq!(
            inflate(&stream[..length], &mut output),
            Err(InflateError::Stored)
        );
    }

    #[test]
    fn a_fixed_block_reads_its_literals_and_its_matches() {
        let mut output = [0; 64];
        let written = inflate(&DEFLATED_FIXED, &mut output).expect("a fixed block");
        assert_eq!(&output[..written], b"RF-7 RF-7 RF-7 cartridge");
    }

    #[test]
    fn a_dynamic_block_builds_the_tables_it_carries() {
        let mut output = [0; 1024];
        let written = inflate(&DEFLATED_DYNAMIC, &mut output).expect("a dynamic block");
        assert_eq!(written, PHRASE.len() * 12);
        for (index, byte) in output[..written].iter().enumerate() {
            assert_eq!(*byte, PHRASE[index % PHRASE.len()], "byte {index}");
        }
    }

    #[test]
    fn a_match_may_overlap_what_it_is_copying() {
        let mut output = [0; 1024];
        let written = inflate(&DEFLATED_OVERLAP, &mut output).expect("a run");
        assert_eq!(written, 700);
        assert!(output[..written].iter().all(|byte| *byte == b'z'));
    }

    #[test]
    fn nothing_is_written_past_the_end_of_the_buffer() {
        for stream in [&DEFLATED_FIXED[..], &DEFLATED_OVERLAP[..]] {
            let mut output = [0; 8];
            assert_eq!(inflate(stream, &mut output), Err(InflateError::Output));
        }
    }

    /// A stream cut anywhere either refuses or produces a prefix of the
    /// right answer. What it may never do is invent the rest.
    #[test]
    fn a_stream_that_stops_early_never_invents_the_rest() {
        const WHOLE: &[u8] = b"RF-7 RF-7 RF-7 cartridge";
        for cut in 0..DEFLATED_FIXED.len() {
            let mut output = [0; 64];
            if let Ok(written) = inflate(&DEFLATED_FIXED[..cut], &mut output) {
                assert!(written <= WHOLE.len(), "{cut} bytes gave {written}");
                assert_eq!(&output[..written], &WHOLE[..written], "{cut} bytes");
            }
        }
    }

    #[test]
    fn a_block_of_a_kind_deflate_does_not_define_is_refused() {
        let mut output = [0; 8];
        assert_eq!(inflate(&[0b111], &mut output), Err(InflateError::Block));
        assert_eq!(inflate(&[], &mut output), Err(InflateError::Truncated));
    }

    #[test]
    fn a_table_that_cannot_exist_is_refused() {
        // Three codes of one bit: one more than a single bit tells apart.
        assert_eq!(Huffman::new(&[1, 1, 1]).err(), Some(InflateError::Code));
        // Two of two bits leave two codes unclaimed, which is broken too.
        assert_eq!(Huffman::new(&[2, 2]).err(), Some(InflateError::Code));
        // Longer than the format allows.
        assert_eq!(Huffman::new(&[16]).err(), Some(InflateError::Code));
        // One code, or none, is the incomplete table the format does allow:
        // a block whose matches share one distance writes exactly that.
        assert!(Huffman::new(&[1]).is_ok());
        assert!(Huffman::new(&[0, 0]).is_ok());
    }
}
