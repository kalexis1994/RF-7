//! The cartridge container as a whole: what round-trips, and what is refused.

use rf7_voice::{
    BULK_DUMP_LENGTH, Cartridge, FACTORY_VOICES, SysexError, VOICE_DUMP_LENGTH,
    VOICES_PER_CARTRIDGE, checksum, decode_bulk_dump, decode_voice_dump, encode_bulk_dump,
    encode_voice_dump, factory_cartridge, factory_voice, printable_name,
};

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
