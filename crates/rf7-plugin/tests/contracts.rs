//! What the host is allowed to do to this plugin, and what it gets back.

use rackforge_plugin_sdk::{MidiEvent, ParameterEvent, Processor};
use rf7_plugin::{MAX_FRAMES, PARAMETER_GAIN, RESOURCE_CARTRIDGE, Rf7Processor, TRANSFER_BYTES};
use rf7_voice::{
    BULK_DUMP_LENGTH, Cartridge, VOICES_PER_CARTRIDGE, Voice, encode_bulk_dump, encode_voice_dump,
    factory_voice,
};

const FRAMES: u32 = 128;

fn prepared() -> Rf7Processor {
    let mut processor = Rf7Processor::default();
    assert!(processor.prepare(48_000.0, MAX_FRAMES, 0, 2));
    processor
}

fn note_on(frame: u32, note: u8, velocity: u8) -> MidiEvent {
    MidiEvent::new(frame, [0x90, note, velocity], 3).expect("a valid note on")
}

fn render(processor: &mut Rf7Processor, midi: &[MidiEvent], blocks: usize) -> Vec<f32> {
    let mut collected = Vec::new();
    for block in 0..blocks {
        let mut output = vec![0.0; FRAMES as usize * 2];
        let events = if block == 0 { midi } else { &[] };
        processor.process(&[], &mut output, events, &[], FRAMES, 0, 2);
        collected.extend_from_slice(&output);
    }
    collected
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
}

#[test]
fn only_a_supported_audio_configuration_is_accepted() {
    let mut processor = Rf7Processor::default();
    assert!(!processor.prepare(48_000.0, 0, 0, 2), "no frames");
    assert!(
        !processor.prepare(48_000.0, MAX_FRAMES + 1, 0, 2),
        "too many frames"
    );
    assert!(
        !processor.prepare(48_000.0, FRAMES, 1, 2),
        "this is not an effect"
    );
    assert!(!processor.prepare(48_000.0, FRAMES, 0, 0), "no output");
    assert!(!processor.prepare(48_000.0, FRAMES, 0, 3), "no such layout");
    assert!(
        !processor.prepare(1.0, FRAMES, 0, 2),
        "unusable sample rate"
    );
    assert!(processor.prepare(44_100.0, FRAMES, 0, 1));
    assert!(processor.prepare(96_000.0, FRAMES, 0, 2));
}

#[test]
fn an_unprepared_plugin_writes_silence_rather_than_refusing() {
    let mut processor = Rf7Processor::default();
    let mut output = vec![1.0; FRAMES as usize * 2];
    processor.process(&[], &mut output, &[note_on(0, 60, 100)], &[], FRAMES, 0, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
}

#[test]
fn a_note_sounds_and_both_channels_carry_it() {
    let mut processor = prepared();
    let rendered = render(&mut processor, &[note_on(0, 60, 100)], 40);
    assert!(peak(&rendered) > 0.0);
    assert!(rendered.iter().all(|sample| sample.is_finite()));
    for pair in rendered.as_chunks::<2>().0 {
        assert_eq!(pair[0], pair[1], "the two channels must be identical");
    }
}

#[test]
fn a_malformed_block_is_silenced_without_a_partial_edit() {
    let mut processor = prepared();
    let good = render(&mut processor, &[note_on(0, 60, 100)], 4);
    assert!(peak(&good) > 0.0);

    let mut processor = prepared();
    let mut output = vec![1.0; FRAMES as usize * 2];
    // A note that starts past the end of the block.
    let past_the_end = [note_on(FRAMES, 60, 100)];
    processor.process(&[], &mut output, &past_the_end, &[], FRAMES, 0, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
    // And the note was not applied behind the silence.
    assert_eq!(peak(&render(&mut processor, &[], 4)), 0.0);

    // Events out of frame order.
    let unordered = [note_on(10, 60, 100), note_on(2, 62, 100)];
    let mut output = vec![1.0; FRAMES as usize * 2];
    processor.process(&[], &mut output, &unordered, &[], FRAMES, 0, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
    assert_eq!(peak(&render(&mut processor, &[], 4)), 0.0);

    // A parameter this plugin does not have.
    let parameters = [ParameterEvent {
        frame: 0,
        index: 7,
        value: 1.0,
    }];
    let mut output = vec![1.0; FRAMES as usize * 2];
    processor.process(&[], &mut output, &[], &parameters, FRAMES, 0, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
}

#[test]
fn a_wrong_length_status_byte_is_refused() {
    let mut processor = prepared();
    let truncated = [MidiEvent::new(0, [0x90, 60, 0], 2).expect("two bytes")];
    let mut output = vec![1.0; FRAMES as usize * 2];
    processor.process(&[], &mut output, &truncated, &[], FRAMES, 0, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
    assert_eq!(peak(&render(&mut processor, &[], 4)), 0.0);
    // A two-byte program change, however, is exactly right.
    let program = [MidiEvent::new(0, [0xc0, 3, 0], 2).expect("two bytes")];
    let mut output = vec![0.0; FRAMES as usize * 2];
    processor.process(&[], &mut output, &program, &[], FRAMES, 0, 2);
    let rendered = render(&mut processor, &[note_on(0, 60, 100)], 20);
    assert!(
        peak(&rendered) > 0.0,
        "the program change should have landed"
    );
}

#[test]
fn the_gain_parameter_is_the_only_one_and_is_bounded() {
    let mut processor = prepared();
    assert_eq!(processor.get_parameter(PARAMETER_GAIN), Some(0.2));
    assert_eq!(processor.get_parameter(1), None);
    assert!(processor.set_parameter(PARAMETER_GAIN, 1.0));
    assert_eq!(processor.get_parameter(PARAMETER_GAIN), Some(1.0));
    assert!(!processor.set_parameter(PARAMETER_GAIN, -0.1));
    assert!(!processor.set_parameter(PARAMETER_GAIN, 2.1));
    assert!(!processor.set_parameter(PARAMETER_GAIN, f64::NAN));
    assert!(!processor.set_parameter(1, 0.5));
    assert_eq!(processor.get_parameter(PARAMETER_GAIN), Some(1.0));
}

#[test]
fn state_round_trips_and_a_broken_state_changes_nothing() {
    let mut processor = prepared();
    processor.set_parameter(PARAMETER_GAIN, 0.75);
    assert!(processor.load_preset("program-05"));
    let mut saved = [0u8; 64];
    let length = processor.save_state(&mut saved).expect("state must fit");
    assert_eq!(length, 20);

    let mut restored = prepared();
    assert!(restored.load_state(&saved[..length]));
    assert_eq!(restored.get_parameter(PARAMETER_GAIN), Some(0.75));

    let mut untouched = prepared();
    untouched.set_parameter(PARAMETER_GAIN, 1.5);
    assert!(!untouched.load_state(&[]), "empty");
    assert!(!untouched.load_state(&saved[..19]), "short");
    let mut wrong_magic = saved;
    wrong_magic[0] = b'X';
    assert!(!untouched.load_state(&wrong_magic[..length]), "wrong magic");
    let mut wrong_version = saved;
    wrong_version[4] = 99;
    assert!(
        !untouched.load_state(&wrong_version[..length]),
        "wrong version"
    );
    let mut wrong_gain = saved;
    wrong_gain[8..16].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(
        !untouched.load_state(&wrong_gain[..length]),
        "gain is not a number"
    );
    let mut wrong_program = saved;
    wrong_program[16..20].copy_from_slice(&99u32.to_le_bytes());
    assert!(
        !untouched.load_state(&wrong_program[..length]),
        "no such program"
    );
    assert_eq!(untouched.get_parameter(PARAMETER_GAIN), Some(1.5));
}

#[test]
fn a_state_saved_before_preparing_still_restores() {
    let mut processor = Rf7Processor::default();
    assert!(processor.load_preset("program-08"));
    let mut saved = [0u8; 64];
    let length = processor.save_state(&mut saved).unwrap();
    let mut later = Rf7Processor::default();
    assert!(later.load_state(&saved[..length]));
    assert!(later.prepare(48_000.0, FRAMES, 0, 2));
    assert!(peak(&render(&mut later, &[note_on(0, 60, 100)], 20)) > 0.0);
}

#[test]
fn only_this_plugins_program_identifiers_are_accepted() {
    let mut processor = prepared();
    assert!(processor.load_preset("program-01"));
    assert!(processor.load_preset("program-32"));
    assert!(!processor.load_preset("program-33"));
    assert!(!processor.load_preset("voice-01"));
    assert!(!processor.load_preset(""));
}

#[test]
fn the_catalog_fits_the_declared_transfer_buffer() {
    let mut processor = prepared();
    let mut buffer = [0u8; TRANSFER_BYTES];
    let length = processor
        .write_program_catalog(&mut buffer)
        .expect("a catalog must always be published");
    assert!(length <= TRANSFER_BYTES);
    let json = std::str::from_utf8(&buffer[..length]).expect("ASCII only");
    assert_eq!(
        json.matches("\"id\":\"program-").count(),
        VOICES_PER_CARTRIDGE
    );
    assert!(json.contains("RF TINES"));
}

#[test]
fn a_cartridge_arrives_in_pieces_and_becomes_the_programs() {
    let mut voices = [factory_voice(1); VOICES_PER_CARTRIDGE];
    voices[0].name = *b"USER TONE ";
    let dump = encode_bulk_dump(&Cartridge::from_voices(voices), 0);

    let mut processor = prepared();
    assert!(processor.begin_resource(RESOURCE_CARTRIDGE, dump.len() as u64));
    for (index, chunk) in dump.chunks(512).enumerate() {
        assert!(processor.write_resource((index * 512) as u64, chunk));
    }
    assert!(processor.end_resource());

    let mut buffer = [0u8; TRANSFER_BYTES];
    let length = processor.write_program_catalog(&mut buffer).unwrap();
    let json = std::str::from_utf8(&buffer[..length]).unwrap();
    assert!(json.contains("USER TONE"), "{json}");
    assert!(!json.contains("RF TINES"));
    assert!(processor.load_preset("program-01"));
    assert!(peak(&render(&mut processor, &[note_on(0, 60, 100)], 20)) > 0.0);
}

#[test]
fn a_single_voice_dump_fills_the_whole_cartridge() {
    let mut voice = Voice::init();
    voice.name = *b"ONE VOICE ";
    voice.operators[0].output_level = 99;
    let dump = encode_voice_dump(&voice, 0);
    let mut processor = prepared();
    assert!(processor.begin_resource(RESOURCE_CARTRIDGE, dump.len() as u64));
    assert!(processor.write_resource(0, &dump));
    assert!(processor.end_resource());
    let mut buffer = [0u8; TRANSFER_BYTES];
    let length = processor.write_program_catalog(&mut buffer).unwrap();
    let json = std::str::from_utf8(&buffer[..length]).unwrap();
    assert_eq!(json.matches("ONE VOICE").count(), VOICES_PER_CARTRIDGE);
}

#[test]
fn a_cartridge_that_does_not_arrive_cleanly_is_refused() {
    let dump = encode_bulk_dump(
        &Cartridge::from_voices([factory_voice(2); VOICES_PER_CARTRIDGE]),
        0,
    );

    let mut processor = prepared();
    assert!(
        !processor.begin_resource("something-else", 16),
        "not our resource"
    );
    assert!(
        !processor.begin_resource(RESOURCE_CARTRIDGE, BULK_DUMP_LENGTH as u64 + 1),
        "larger than any cartridge"
    );
    assert!(!processor.write_resource(0, &dump), "no delivery was begun");

    assert!(processor.begin_resource(RESOURCE_CARTRIDGE, dump.len() as u64));
    assert!(
        !processor.write_resource(64, &dump[64..]),
        "a gap is not sequential"
    );
    assert!(!processor.end_resource(), "the delivery already failed");

    // Bytes that are not a dump at all.
    assert!(processor.begin_resource(RESOURCE_CARTRIDGE, 8));
    assert!(processor.write_resource(0, b"not sysex"[..8].as_ref()));
    assert!(!processor.end_resource());

    // A dump with one payload byte edited, so only the checksum can catch it.
    let mut edited = dump;
    edited[100] ^= 1;
    assert!(processor.begin_resource(RESOURCE_CARTRIDGE, edited.len() as u64));
    assert!(processor.write_resource(0, &edited));
    assert!(!processor.end_resource());

    // Through all of that the factory programs are still the ones offered.
    let mut buffer = [0u8; TRANSFER_BYTES];
    let length = processor.write_program_catalog(&mut buffer).unwrap();
    assert!(
        std::str::from_utf8(&buffer[..length])
            .unwrap()
            .contains("RF TINES")
    );
}

#[test]
fn reset_stops_every_sounding_note() {
    let mut processor = prepared();
    let mut midi = Vec::new();
    for note in 60..70 {
        midi.push(note_on(0, note, 100));
    }
    assert!(peak(&render(&mut processor, &midi, 4)) > 0.0);
    processor.reset();
    assert_eq!(peak(&render(&mut processor, &[], 4)), 0.0);
}
