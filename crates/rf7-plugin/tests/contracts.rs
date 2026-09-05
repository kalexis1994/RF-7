//! What the host is allowed to do to this plugin, and what it gets back.

use rackforge_plugin_sdk::{MidiEvent, ParameterEvent, Processor};
use rf7_plugin::{
    MAX_FRAMES, PARAMETER_COUNT, PARAMETER_GAIN, RESOURCE_CARTRIDGE, Rf7Processor, TRANSFER_BYTES,
    parameters,
};
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
fn every_declared_parameter_is_readable_writable_and_bounded() {
    let mut processor = prepared();
    assert_eq!(processor.get_parameter(PARAMETER_COUNT as u32), None);
    assert_eq!(processor.get_parameter(u32::MAX), None);
    assert!(!processor.set_parameter(PARAMETER_COUNT as u32, 0.0));

    for (index, parameter) in parameters::PARAMETERS.iter().enumerate() {
        let index = index as u32;
        assert_eq!(
            processor.get_parameter(index),
            Some(parameter.default),
            "{} does not start at its declared default",
            parameter.id
        );
        assert!(
            processor.set_parameter(index, parameter.maximum),
            "{}",
            parameter.id
        );
        assert_eq!(processor.get_parameter(index), Some(parameter.maximum));
        assert!(!processor.set_parameter(index, parameter.maximum + 0.5));
        assert!(!processor.set_parameter(index, parameter.minimum - 0.5));
        assert!(!processor.set_parameter(index, f64::NAN));
        assert_eq!(
            processor.get_parameter(index),
            Some(parameter.maximum),
            "{} moved on a refused write",
            parameter.id
        );
        assert!(processor.set_parameter(index, parameter.default));
    }
}

#[test]
fn the_gain_parameter_still_scales_the_output() {
    let mut loud = prepared();
    loud.set_parameter(PARAMETER_GAIN, 1.0);
    let full = peak(&render(&mut loud, &[note_on(0, 60, 100)], 20));
    let mut quiet = prepared();
    quiet.set_parameter(PARAMETER_GAIN, 0.5);
    let half = peak(&render(&mut quiet, &[note_on(0, 60, 100)], 20));
    assert!(
        (half * 2.0 - full).abs() < full * 0.02,
        "{half} against {full}"
    );
}

#[test]
fn muting_every_operator_silences_the_instrument() {
    let mut processor = prepared();
    for operator in 0..6 {
        assert!(processor.set_parameter(parameters::OPERATOR_FIRST + operator, 0.0));
    }
    assert_eq!(
        peak(&render(&mut processor, &[note_on(0, 60, 100)], 20)),
        0.0
    );
    assert!(processor.set_parameter(parameters::OPERATOR_FIRST, 1.0));
    assert!(peak(&render(&mut processor, &[note_on(0, 62, 100)], 20)) > 0.0);
}

#[test]
fn aftertouch_reaches_the_engine_at_both_midi_widths() {
    // Opened all the way to pitch, so pressure is unmistakable in the output.
    let sounded = |pressure: Option<MidiEvent>| {
        let mut processor = prepared();
        processor.set_parameter(parameters::AFTERTOUCH_RANGE, 1.0);
        let mut midi = vec![note_on(0, 60, 100)];
        midi.extend(pressure);
        render(&mut processor, &midi, 20)
    };
    let idle = sounded(None);
    let pressed = sounded(Some(
        MidiEvent::new(0, [0xd0, 127, 0], 2).expect("channel pressure is two bytes"),
    ));
    assert_ne!(idle, pressed, "channel pressure should be heard");

    // A three-byte channel pressure is malformed and silences the block.
    let mut processor = prepared();
    let malformed = [MidiEvent::new(0, [0xd0, 127, 0], 3).expect("three bytes")];
    let mut output = vec![1.0; FRAMES as usize * 2];
    processor.process(&[], &mut output, &malformed, &[], FRAMES, 0, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
}

#[test]
fn state_round_trips_and_a_broken_state_changes_nothing() {
    let expected = 16 + PARAMETER_COUNT * 8;
    let mut processor = prepared();
    processor.set_parameter(PARAMETER_GAIN, 0.75);
    processor.set_parameter(parameters::BEND_RANGE, 12.0);
    processor.set_parameter(parameters::BRIGHTNESS, 1.5);
    processor.set_parameter(parameters::OPERATOR_FIRST + 2, 0.0);
    assert!(processor.load_preset("program-05"));
    let mut saved = [0u8; 512];
    let length = processor.save_state(&mut saved).expect("state must fit");
    assert_eq!(length, expected);
    // A buffer that cannot hold the whole state writes none of it.
    assert_eq!(processor.save_state(&mut [0u8; 8]), None);

    let mut restored = prepared();
    assert!(restored.load_state(&saved[..length]));
    assert_eq!(restored.get_parameter(PARAMETER_GAIN), Some(0.75));
    assert_eq!(restored.get_parameter(parameters::BEND_RANGE), Some(12.0));
    assert_eq!(restored.get_parameter(parameters::BRIGHTNESS), Some(1.5));
    assert_eq!(
        restored.get_parameter(parameters::OPERATOR_FIRST + 2),
        Some(0.0)
    );

    let mut untouched = prepared();
    untouched.set_parameter(PARAMETER_GAIN, 1.5);
    let refused: [(&str, Vec<u8>); 6] = [
        ("empty", Vec::new()),
        ("short", saved[..length - 1].to_vec()),
        ("wrong magic", {
            let mut bytes = saved[..length].to_vec();
            bytes[0] = b'X';
            bytes
        }),
        ("unknown version", {
            let mut bytes = saved[..length].to_vec();
            bytes[4] = 99;
            bytes
        }),
        ("impossible count", {
            let mut bytes = saved[..length].to_vec();
            bytes[12..16].copy_from_slice(&99u32.to_le_bytes());
            bytes
        }),
        ("a value out of its range", {
            let mut bytes = saved[..length].to_vec();
            bytes[16..24].copy_from_slice(&99.0f64.to_le_bytes());
            bytes
        }),
    ];
    for (reason, bytes) in refused {
        assert!(!untouched.load_state(&bytes), "{reason} was accepted");
    }
    assert_eq!(untouched.get_parameter(PARAMETER_GAIN), Some(1.5));
}

#[test]
fn a_state_written_by_the_first_version_still_opens() {
    // Twenty bytes: magic, version 1, a gain and a program. Everything the
    // first release never had comes back at its default.
    let mut first = [0u8; 20];
    first[..4].copy_from_slice(b"RF7A");
    first[4..8].copy_from_slice(&1u32.to_le_bytes());
    first[8..16].copy_from_slice(&0.6f64.to_le_bytes());
    first[16..20].copy_from_slice(&4u32.to_le_bytes());

    let mut processor = prepared();
    processor.set_parameter(parameters::BRIGHTNESS, 2.0);
    assert!(processor.load_state(&first));
    assert_eq!(processor.get_parameter(PARAMETER_GAIN), Some(0.6));
    assert_eq!(processor.get_parameter(parameters::BRIGHTNESS), Some(1.0));
    assert!(peak(&render(&mut processor, &[note_on(0, 60, 100)], 20)) > 0.0);

    // The same header with a program that does not exist is still refused.
    let mut broken = first;
    broken[16..20].copy_from_slice(&99u32.to_le_bytes());
    assert!(!processor.load_state(&broken));
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
