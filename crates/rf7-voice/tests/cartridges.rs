//! The cartridge container as a whole: what round-trips, and what is refused.

use rf7_voice::{
    BULK_DUMP_LENGTH, Cartridge, FACTORY_VOICES, Library, MAX_VOICES, RAW_BANK_LENGTH, SysexError,
    VOICE_DUMP_LENGTH, VOICES_PER_CARTRIDGE, checksum, decode_bulk_dump, decode_library,
    decode_voice_dump, encode_bulk_dump, encode_packed, encode_voice_dump, factory_library,
    factory_voice, printable_name,
};

/// The eight factory voices repeated to fill a cartridge, so a test has
/// thirty-two distinguishable slots without any INIT padding.
fn factory_cartridge() -> Cartridge {
    let mut voices = [factory_voice(0); VOICES_PER_CARTRIDGE];
    for (slot, voice) in voices.iter_mut().enumerate() {
        *voice = factory_voice(slot % FACTORY_VOICES);
    }
    Cartridge::from_voices(voices)
}

/// One bank as a chip holds it: thirty-two packed voices, no framing.
fn raw_bank(first: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for slot in 0..VOICES_PER_CARTRIDGE {
        bytes.extend_from_slice(&encode_packed(&factory_voice(
            (first + slot) % FACTORY_VOICES,
        )));
    }
    bytes
}

#[test]
fn a_cartridge_survives_the_round_trip_through_system_exclusive() {
    let original = factory_cartridge();
    let dump = encode_bulk_dump(&original, 0);
    assert_eq!(dump.len(), BULK_DUMP_LENGTH);
    let decoded = decode_bulk_dump(&dump).expect("our own dump must decode");
    assert_eq!(decoded.voices(), original.voices());
    assert!(decoded.corrections().is_clean());
    assert_eq!(printable_name(&decoded.voice(0).unwrap().name), "RF TINES");
}

#[test]
fn the_dump_channel_does_not_change_the_voices() {
    let cartridge = factory_cartridge();
    let first = decode_bulk_dump(&encode_bulk_dump(&cartridge, 0)).unwrap();
    let last = decode_bulk_dump(&encode_bulk_dump(&cartridge, 15)).unwrap();
    assert_eq!(first.voices(), last.voices());
}

#[test]
fn a_single_voice_dump_carries_the_same_voice() {
    for index in 0..FACTORY_VOICES {
        let voice = factory_voice(index);
        let dump = encode_voice_dump(&voice, 3);
        assert_eq!(dump.len(), VOICE_DUMP_LENGTH);
        let decoded = decode_voice_dump(&dump).expect("our own dump must decode");
        assert_eq!(decoded.voice, voice);
        assert!(decoded.corrections.is_clean());
    }
}

#[test]
fn the_two_container_lengths_are_not_interchangeable() {
    let cartridge = encode_bulk_dump(&factory_cartridge(), 0);
    let voice = encode_voice_dump(&factory_voice(0), 0);
    assert!(matches!(
        decode_voice_dump(&cartridge),
        Err(SysexError::Length { .. })
    ));
    assert!(matches!(
        decode_bulk_dump(&voice),
        Err(SysexError::Length { .. })
    ));
    assert!(matches!(
        decode_bulk_dump(&[]),
        Err(SysexError::Length { found: 0 })
    ));
}

#[test]
fn a_damaged_dump_is_refused_rather_than_half_read() {
    let good = encode_bulk_dump(&factory_cartridge(), 0);

    let mut wrong_manufacturer = good;
    wrong_manufacturer[1] = 0x41;
    assert_eq!(
        decode_bulk_dump(&wrong_manufacturer),
        Err(SysexError::Header)
    );

    let mut wrong_format = good;
    wrong_format[3] = 0x00;
    assert_eq!(decode_bulk_dump(&wrong_format), Err(SysexError::Header));

    let mut unterminated = good;
    unterminated[BULK_DUMP_LENGTH - 1] = 0x00;
    assert_eq!(decode_bulk_dump(&unterminated), Err(SysexError::Header));

    let mut eight_bit = good;
    eight_bit[100] = 0x80;
    assert_eq!(
        decode_bulk_dump(&eight_bit),
        Err(SysexError::DataBit { offset: 100 })
    );

    // A payload edit that stays seven-bit is caught by the checksum alone.
    let mut edited = good;
    edited[10] ^= 0x01;
    assert!(matches!(
        decode_bulk_dump(&edited),
        Err(SysexError::Checksum { .. })
    ));
}

#[test]
fn the_checksum_is_the_seven_bit_negated_sum() {
    assert_eq!(checksum(&[]), 0);
    assert_eq!(checksum(&[0x01]), 0x7f);
    assert_eq!(checksum(&[0x40, 0x40]), 0x00);
    let payload = [0x7f; 4096];
    assert_eq!(
        (u32::from(checksum(&payload)) + payload.iter().map(|byte| u32::from(*byte)).sum::<u32>())
            % 128,
        0
    );
}

#[test]
fn a_cartridge_of_unwritten_memory_still_opens() {
    // Every payload byte 0x7f, with the checksum that implies. Real cartridges
    // arrive like this, and losing all 32 voices over it would be the wrong
    // trade: the corrections count says how little was trustworthy.
    let mut dump = [0x7fu8; BULK_DUMP_LENGTH];
    dump[..6].copy_from_slice(&[0xf0, 0x43, 0x00, 0x09, 0x20, 0x00]);
    dump[BULK_DUMP_LENGTH - 1] = 0xf7;
    dump[BULK_DUMP_LENGTH - 2] = checksum(&dump[6..BULK_DUMP_LENGTH - 2]);
    let cartridge = decode_bulk_dump(&dump).expect("a valid frame must decode");
    assert!(!cartridge.corrections().is_clean());
    assert_eq!(cartridge.voices().len(), VOICES_PER_CARTRIDGE);
    for index in 0..VOICES_PER_CARTRIDGE {
        assert!(!cartridge.voice_corrections(index).is_clean());
        assert_eq!(cartridge.voice(index).unwrap().algorithm, 31);
    }
}

#[test]
fn a_cartridge_built_from_voices_reports_no_corrections() {
    let voices = [factory_voice(0); VOICES_PER_CARTRIDGE];
    let cartridge = Cartridge::from_voices(voices);
    assert!(cartridge.corrections().is_clean());
    assert_eq!(cartridge.voice(31), Some(&factory_voice(0)));
}

#[test]
fn a_raw_bank_is_read_without_a_checksum_to_lean_on() {
    // A chip image has no room for a checksum, so shape is all there is: the
    // right length, and every byte seven-bit.
    let library = decode_library(&raw_bank(0)).expect("a raw bank must open");
    assert_eq!(library.len(), VOICES_PER_CARTRIDGE);
    assert_eq!(library.found(), VOICES_PER_CARTRIDGE);
    assert!(library.corrections().is_clean());
    assert_eq!(printable_name(&library.voice(0).unwrap().name), "RF TINES");
}

#[test]
fn two_raw_banks_become_sixty_four_voices_in_file_order() {
    let mut bytes = raw_bank(0);
    bytes.extend_from_slice(&raw_bank(1));
    let library = decode_library(&bytes).expect("two banks must open");
    assert_eq!(library.len(), 64);
    assert_eq!(library.voice(0), Some(&factory_voice(0)));
    assert_eq!(library.voice(32), Some(&factory_voice(1)));
}

#[test]
fn concatenated_bulk_dumps_are_read_end_to_end() {
    // Collections are distributed as one file holding several dumps.
    let mut bytes = encode_bulk_dump(&factory_cartridge(), 0).to_vec();
    bytes.extend_from_slice(&encode_bulk_dump(
        &Cartridge::from_voices([factory_voice(3); VOICES_PER_CARTRIDGE]),
        9,
    ));
    let library = decode_library(&bytes).expect("two dumps must open");
    assert_eq!(library.len(), 64);
    assert_eq!(library.voice(0), Some(&factory_voice(0)));
    assert_eq!(library.voice(32), Some(&factory_voice(3)));
    // One damaged dump refuses the whole file rather than half-loading it.
    bytes[BULK_DUMP_LENGTH + 100] ^= 1;
    assert!(matches!(
        decode_library(&bytes),
        Err(SysexError::Checksum { .. })
    ));
}

#[test]
fn a_single_voice_dump_is_a_library_of_one() {
    let bytes = encode_voice_dump(&factory_voice(2), 0);
    let library = decode_library(&bytes).expect("a voice dump must open");
    assert_eq!(library.len(), 1);
    assert_eq!(library.voice(0), Some(&factory_voice(2)));
    assert_eq!(library.voice(1), None);
}

/// One bay's voices are one stretch of the library: replacing them leaves
/// every other bay where it was.
#[test]
fn splicing_replaces_one_stretch_and_moves_the_rest() {
    let library = |first: usize, count: usize| {
        let voices: Vec<_> = (0..count)
            .map(|index| factory_voice(first + index))
            .collect();
        Library::from_voices(&voices)
    };
    let mut rack = library(0, 3);
    // A second bay after the first.
    assert_eq!(rack.splice(3, 0, &library(10, 2)), 2);
    assert_eq!(rack.len(), 5);
    assert_eq!(rack.voice(3), Some(&factory_voice(10)));
    // The first bay gets a bigger cartridge; the second moves along.
    assert_eq!(rack.splice(0, 3, &library(20, 5)), 5);
    assert_eq!(rack.len(), 7);
    assert_eq!(rack.voice(0), Some(&factory_voice(20)));
    assert_eq!(rack.voice(5), Some(&factory_voice(10)));
    // And emptied, which is what taking a cartridge out does.
    assert_eq!(rack.splice(0, 5, &Library::from_voices(&[])), 0);
    assert_eq!(rack.len(), 2);
    assert_eq!(rack.voice(0), Some(&factory_voice(10)));

    // Past the cap the tail falls off rather than the insertion.
    let mut full = library(0, 4);
    let long: Vec<_> = (0..MAX_VOICES)
        .map(|index| factory_voice(index % FACTORY_VOICES))
        .collect();
    assert_eq!(
        full.splice(2, 0, &Library::from_voices(&long)),
        MAX_VOICES - 2
    );
    assert_eq!(full.len(), MAX_VOICES);
    assert_eq!(full.voice(2), Some(&factory_voice(0)));
}

#[test]
fn more_voices_than_rf7_offers_are_counted_and_capped() {
    const BANKS: usize = 10;
    let mut bytes = Vec::new();
    for bank in 0..BANKS {
        bytes.extend_from_slice(&raw_bank(bank));
    }
    let library = decode_library(&bytes).expect("ten banks must open");
    assert_eq!(library.len(), MAX_VOICES);
    assert_eq!(library.found(), BANKS * VOICES_PER_CARTRIDGE);
    assert_eq!(library.voice(MAX_VOICES), None);
}

#[test]
fn a_length_rf7_does_not_recognise_is_refused() {
    for length in [1usize, 100, 4095, 4097, 4103, 4105, 8191] {
        assert!(
            matches!(
                decode_library(&vec![0u8; length]),
                Err(SysexError::Length { .. })
            ),
            "{length} bytes was accepted"
        );
    }
    let mut bank = raw_bank(0);
    bank[500] = 0x80;
    assert_eq!(
        decode_library(&bank),
        Err(SysexError::DataBit { offset: 500 })
    );
}

#[test]
fn a_library_reports_what_it_had_to_clamp() {
    // Unwritten chip memory: every byte set, which is a legal bank shape.
    let library = decode_library(&[0x7f; RAW_BANK_LENGTH]).expect("a valid shape");
    assert_eq!(library.len(), VOICES_PER_CARTRIDGE);
    assert!(!library.corrections().is_clean());
    assert!(!library.voice_corrections(0).is_clean());
    assert!(library.voice_corrections(VOICES_PER_CARTRIDGE).is_clean());
}

#[test]
fn the_factory_library_needs_no_file_at_all() {
    let library = factory_library();
    assert_eq!(library.len(), FACTORY_VOICES);
    assert!(library.corrections().is_clean());
    assert_eq!(
        Library::default().len(),
        1,
        "never empty, never a special case"
    );
}
