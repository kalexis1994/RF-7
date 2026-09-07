//! Cartridges inside a ZIP, read the way the collections are published.
//!
//! The archives people download hold the same bank four times over — a
//! System Exclusive dump, a MIDI file, and two editors' own formats — in a
//! folder, sometimes with a note beside them. What matters is which of those
//! RF-7 takes, in what order, and what it does with everything else, so the
//! archives here are built to look like the real ones.
//!
//! They are built here rather than committed: no voice data from any other
//! instrument belongs in this repository, so every bank below is RF-7's own.

use rf7_voice::zip::crc32;
use rf7_voice::{
    Cartridge, FACTORY_VOICES, MAX_VOICES, SCRATCH_BYTES, VOICES_PER_CARTRIDGE, Voice,
    decode_cartridge, encode_bulk_dump, factory_voice, printable_name,
};

/// A whole bank of RF-7's own voices, as a bulk dump, starting from the
/// factory voice at `first` so two banks can be told apart by their names.
fn bank(first: usize) -> Vec<u8> {
    let voices: [Voice; VOICES_PER_CARTRIDGE] =
        core::array::from_fn(|index| factory_voice((first + index) % FACTORY_VOICES));
    encode_bulk_dump(&Cartridge::from_voices(voices), 0).to_vec()
}

/// The name of one factory voice, as the library will print it.
fn voice_name(index: usize) -> String {
    printable_name(&factory_voice(index % FACTORY_VOICES).name).to_owned()
}

/// A whole bank of zeros, compressed. Every byte is inside the seven-bit
/// range a chip image must be, so it reads as thirty-two voices.
const DEFLATED_ZERO_BANK: [u8; 20] = [
    0xed, 0xc1, 0x01, 0x0d, 0x00, 0x00, 0x00, 0xc2, 0xa0, 0xf7, 0x4f, 0x6d, 0x0f, 0x07, 0x14, 0x00,
    0x00, 0x00, 0xf0, 0x6e,
];
const ZERO_BANK_LENGTH: u32 = 4096;
const ZERO_BANK_CRC: u32 = 0xc71c_0011;

const STORED: u16 = 0;
const DEFLATE: u16 = 8;
const ENCRYPTED: u16 = 1;

/// An archive, written the way an archiver writes one: every entry with a
/// header in front of it, then a directory, then the record that points at
/// the directory.
#[derive(Default)]
struct Zip {
    bytes: Vec<u8>,
    directory: Vec<u8>,
    entries: u16,
}

impl Zip {
    /// An entry kept as it is, which needs no compressor here.
    fn stored(mut self, name: &str, data: &[u8]) -> Self {
        let crc = crc32(data);
        self.push(name, data, STORED, crc, data.len() as u32, 0);
        self
    }

    /// An entry compressed elsewhere, handed over with what it comes to.
    fn deflated(mut self, name: &str, data: &[u8], size: u32, crc: u32) -> Self {
        self.push(name, data, DEFLATE, crc, size, 0);
        self
    }

    fn flagged(mut self, name: &str, data: &[u8], flags: u16) -> Self {
        let crc = crc32(data);
        self.push(name, data, STORED, crc, data.len() as u32, flags);
        self
    }

    fn method(mut self, name: &str, data: &[u8], method: u16) -> Self {
        let crc = crc32(data);
        self.push(name, data, method, crc, data.len() as u32, 0);
        self
    }

    fn push(&mut self, name: &str, payload: &[u8], method: u16, crc: u32, size: u32, flags: u16) {
        let offset = self.bytes.len() as u32;
        let name = name.as_bytes();
        let compressed = payload.len() as u32;
        let mut header = Vec::new();
        header.extend_from_slice(b"PK\x03\x04");
        header.extend_from_slice(&20_u16.to_le_bytes());
        header.extend_from_slice(&flags.to_le_bytes());
        header.extend_from_slice(&method.to_le_bytes());
        header.extend_from_slice(&0_u32.to_le_bytes());
        header.extend_from_slice(&crc.to_le_bytes());
        header.extend_from_slice(&compressed.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(name);
        self.bytes.extend_from_slice(&header);
        self.bytes.extend_from_slice(payload);

        let mut record = Vec::new();
        record.extend_from_slice(b"PK\x01\x02");
        record.extend_from_slice(&20_u16.to_le_bytes());
        record.extend_from_slice(&20_u16.to_le_bytes());
        record.extend_from_slice(&flags.to_le_bytes());
        record.extend_from_slice(&method.to_le_bytes());
        record.extend_from_slice(&0_u32.to_le_bytes());
        record.extend_from_slice(&crc.to_le_bytes());
        record.extend_from_slice(&compressed.to_le_bytes());
        record.extend_from_slice(&size.to_le_bytes());
        record.extend_from_slice(&(name.len() as u16).to_le_bytes());
        record.extend_from_slice(&0_u16.to_le_bytes());
        record.extend_from_slice(&0_u16.to_le_bytes());
        record.extend_from_slice(&0_u16.to_le_bytes());
        record.extend_from_slice(&0_u16.to_le_bytes());
        record.extend_from_slice(&0_u32.to_le_bytes());
        record.extend_from_slice(&offset.to_le_bytes());
        record.extend_from_slice(name);
        self.directory.extend_from_slice(&record);
        self.entries += 1;
    }

    fn finish(mut self) -> Vec<u8> {
        let offset = self.bytes.len() as u32;
        let size = self.directory.len() as u32;
        self.bytes.extend_from_slice(&self.directory);
        self.bytes.extend_from_slice(b"PK\x05\x06");
        self.bytes.extend_from_slice(&0_u32.to_le_bytes());
        self.bytes.extend_from_slice(&self.entries.to_le_bytes());
        self.bytes.extend_from_slice(&self.entries.to_le_bytes());
        self.bytes.extend_from_slice(&size.to_le_bytes());
        self.bytes.extend_from_slice(&offset.to_le_bytes());
        self.bytes.extend_from_slice(&0_u16.to_le_bytes());
        self.bytes
    }
}

fn read(bytes: &[u8]) -> Result<rf7_voice::Library, rf7_voice::CartridgeError> {
    decode_cartridge(bytes, &mut vec![0; SCRATCH_BYTES])
}

/// A published cartridge: two banks, each in four formats, in a folder.
#[test]
fn the_dumps_are_read_in_name_order_and_the_other_formats_are_left_alone() {
    let (first, second) = (bank(0), bank(16));
    // Bank B is written first, and the copies of bank A carry the same
    // voices under the other formats' names — as an archiver and a
    // publisher respectively would leave them.
    let archive = Zip::default()
        .stored("ROM1/ROM1B.syx", &second)
        .stored("ROM1/ROM1B.mid", &second)
        .stored("ROM1/ROM1A.syx", &first)
        .stored("ROM1/ROM1A.mid", &first)
        .stored("ROM1/ROM1A.VBk", &first)
        .stored("ROM1/ROM1A.LIB", &first)
        .stored("ROM1/readme.txt", b"Master group. Do not distribute.")
        .finish();
    let library = read(&archive).expect("a published cartridge");
    assert_eq!(library.len(), 2 * VOICES_PER_CARTRIDGE);
    assert_eq!(library.corrections().0, 0);
    // A first, then B, whatever order they were written in.
    assert_eq!(printable_name(&library.voices()[0].name), voice_name(0));
    assert_eq!(
        printable_name(&library.voices()[VOICES_PER_CARTRIDGE].name),
        voice_name(16)
    );
}

/// The same file twice is one cartridge, not two.
#[test]
fn a_second_copy_of_a_file_already_taken_is_passed_over() {
    let bank = bank(0);
    let archive = Zip::default()
        .stored("banks/ROM1A.syx", &bank)
        .stored("copies/ROM1A.syx", &bank)
        .finish();
    assert_eq!(read(&archive).unwrap().len(), VOICES_PER_CARTRIDGE);
}

#[test]
fn a_compressed_entry_is_decompressed_and_checked() {
    let archive = Zip::default()
        .deflated(
            "bank.syx",
            &DEFLATED_ZERO_BANK,
            ZERO_BANK_LENGTH,
            ZERO_BANK_CRC,
        )
        .finish();
    assert_eq!(read(&archive).unwrap().len(), VOICES_PER_CARTRIDGE);

    // The same entry with a checksum that does not match its contents is
    // passed over, and an archive of nothing else has nothing to install.
    let broken = Zip::default()
        .deflated("bank.syx", &DEFLATED_ZERO_BANK, ZERO_BANK_LENGTH, 0)
        .finish();
    assert_eq!(read(&broken), Err(rf7_voice::CartridgeError::Empty));

    // One broken file does not lose the good ones beside it.
    let mixed = Zip::default()
        .deflated("a.syx", &DEFLATED_ZERO_BANK, ZERO_BANK_LENGTH, 0)
        .stored("b.syx", &bank(0))
        .finish();
    assert_eq!(read(&mixed).unwrap().len(), VOICES_PER_CARTRIDGE);
}

/// Chip images and dumps are both cartridges, but an archive holding both
/// is one cartridge published twice, so only the dumps are read.
#[test]
fn the_extension_decides_which_copies_are_read() {
    let bank = bank(0);
    let raw = &bank[6..bank.len() - 2];
    let both = Zip::default()
        .stored("ROM1.bin", raw)
        .stored("ROM1.syx", &bank)
        .finish();
    assert_eq!(read(&both).unwrap().len(), VOICES_PER_CARTRIDGE);

    // With no dump to prefer, the chip image is what there is.
    let images = Zip::default().stored("ROM1.bin", raw).finish();
    assert_eq!(read(&images).unwrap().len(), VOICES_PER_CARTRIDGE);

    // And with neither, every file is a candidate, so a cartridge saved
    // under a name nobody agreed on still installs.
    let unnamed = Zip::default()
        .stored("notes.txt", b"the master group")
        .stored("cartridge", &bank)
        .finish();
    assert_eq!(read(&unnamed).unwrap().len(), VOICES_PER_CARTRIDGE);
}

#[test]
fn an_archive_with_no_cartridge_in_it_is_refused() {
    let empty = Zip::default()
        .stored("readme.txt", b"nothing to see")
        .finish();
    assert_eq!(read(&empty), Err(rf7_voice::CartridgeError::Empty));
    assert_eq!(
        read(&Zip::default().finish()),
        Err(rf7_voice::CartridgeError::Empty)
    );
}

#[test]
fn what_cannot_be_read_is_refused_rather_than_guessed_at() {
    let bank = bank(0);
    // A password, and a compression method from another decade.
    for archive in [
        Zip::default().flagged("a.syx", &bank, ENCRYPTED).finish(),
        Zip::default().method("a.syx", &bank, 12).finish(),
    ] {
        assert_eq!(read(&archive), Err(rf7_voice::CartridgeError::Empty));
    }

    // A directory that points outside the file.
    let mut broken = Zip::default().stored("a.syx", &bank).finish();
    let end = broken.len() - 6;
    broken[end..end + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read(&broken).is_err());

    // A file cut in half keeps its directory but loses its contents.
    let whole = Zip::default().stored("a.syx", &bank).finish();
    assert!(read(&whole[..whole.len() / 2]).is_err());
}

/// The scratch is what bounds how much one entry may expand into.
#[test]
fn a_file_larger_than_the_scratch_is_passed_over() {
    let archive = Zip::default().stored("a.syx", &bank(0)).finish();
    assert_eq!(
        decode_cartridge(&archive, &mut [0; 128]),
        Err(rf7_voice::CartridgeError::Empty)
    );
    assert!(decode_cartridge(&archive, &mut vec![0; SCRATCH_BYTES]).is_ok());
}

/// More voices than RF-7 can hold are counted and then left behind.
#[test]
fn an_archive_of_more_banks_than_fit_keeps_what_it_can() {
    const BANKS: usize = 10;
    let mut archive = Zip::default();
    for index in 0..BANKS {
        archive = archive.stored(&format!("bank{index}.syx"), &bank(index * 3));
    }
    let library = read(&archive.finish()).unwrap();
    assert_eq!(library.len(), MAX_VOICES);
    assert_eq!(library.found(), BANKS * VOICES_PER_CARTRIDGE);
}

/// Everything that is not an archive still reads exactly as it did.
#[test]
fn a_cartridge_that_is_not_an_archive_reads_as_itself() {
    assert_eq!(read(&bank(0)).unwrap().len(), VOICES_PER_CARTRIDGE);
    assert!(read(&[]).is_err());
    assert!(read(b"PK\x03\x04 and nothing else").is_err());
}
