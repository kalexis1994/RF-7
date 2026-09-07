//! The program-editing contract, end to end, with the host's own validators
//! reading everything the plugin emits.

use rackforge_plugin_api::PresetCatalog;
use rackforge_plugin_sdk::{MidiEvent, ParallelProcessor};
use rackforge_program_api::{
    PreparedProgram, ProgramDocument, ProgramEditRequest, ProgramEditorFieldKind,
    ProgramEditorPage, ProgramEditorValue, ProgramEditorView, ProgramFieldEditRequest,
};
mod common;
/// The plugin driven the way a host drives it, over buffers of its own.
use common::Host as Rf7Processor;
use rf7_plugin::{RESOURCE_CARTRIDGE, TRANSFER_BYTES};
use rf7_voice::{
    VOICES_PER_CARTRIDGE, decode_library, decode_voice_dump, encode_packed, factory_voice,
};

const FRAMES: u32 = 128;

fn prepared() -> Rf7Processor {
    let mut processor = Rf7Processor::default();
    assert!(processor.prepare(48_000.0, 4096, 0, 2));
    processor
}

fn catalog(processor: &mut Rf7Processor) -> PresetCatalog {
    let mut buffer = [0u8; TRANSFER_BYTES];
    let length = processor
        .write_program_catalog(&mut buffer)
        .expect("a catalog must always be published");
    let catalog: PresetCatalog = serde_json::from_slice(&buffer[..length]).expect("valid JSON");
    catalog.validate().expect("the host accepts the catalog");
    catalog
}

fn begin(processor: &mut Rf7Processor, program_id: Option<&str>) -> PreparedProgram {
    let request = ProgramEditRequest::new(program_id.map(str::to_owned));
    request.validate().unwrap();
    let source = serde_json::to_vec(&request).unwrap();
    let mut buffer = vec![0u8; TRANSFER_BYTES];
    let length = processor
        .begin_program_edit(&source, &mut buffer)
        .expect("the editor opens");
    let prepared: PreparedProgram = serde_json::from_slice(&buffer[..length]).unwrap();
    prepared
        .validate()
        .expect("the host accepts the prepared program");
    prepared
}

fn view(processor: &mut Rf7Processor, document: &ProgramDocument) -> ProgramEditorView {
    let source = serde_json::to_vec(document).unwrap();
    let mut buffer = vec![0u8; TRANSFER_BYTES];
    let length = processor
        .program_editor_view(&source, &mut buffer)
        .expect("the view is published");
    let view: ProgramEditorView = serde_json::from_slice(&buffer[..length]).unwrap();
    view.validate().expect("the host accepts the editor view");
    view
}

fn apply(
    processor: &mut Rf7Processor,
    document: &ProgramDocument,
    field_id: &str,
    value: ProgramEditorValue,
) -> Option<PreparedProgram> {
    let request = ProgramFieldEditRequest {
        schema_version: rackforge_program_api::PROGRAM_EDIT_SCHEMA_VERSION,
        document: document.clone(),
        field_id: field_id.to_owned(),
        value,
    };
    request.validate().unwrap();
    let source = serde_json::to_vec(&request).unwrap();
    let mut buffer = vec![0u8; TRANSFER_BYTES];
    let length = processor.apply_program_edit(&source, &mut buffer)?;
    let prepared: PreparedProgram = serde_json::from_slice(&buffer[..length]).unwrap();
    prepared
        .validate()
        .expect("the host accepts the edited program");
    Some(prepared)
}

fn save(processor: &mut Rf7Processor, document: &ProgramDocument) -> PreparedProgram {
    let source = serde_json::to_vec(document).unwrap();
    let mut buffer = vec![0u8; TRANSFER_BYTES];
    let length = processor
        .prepare_program_save(&source, &mut buffer)
        .expect("the save is prepared");
    let prepared: PreparedProgram = serde_json::from_slice(&buffer[..length]).unwrap();
    prepared.validate().unwrap();
    assert!(
        processor.install_program(&serde_json::to_vec(&prepared).unwrap()),
        "the program installs"
    );
    prepared
}

fn preview(processor: &mut Rf7Processor, prepared: &PreparedProgram) -> bool {
    processor.preview_program(&serde_json::to_vec(prepared).unwrap())
}

fn note_on(note: u8) -> MidiEvent {
    MidiEvent::new(0, [0x90, note, 100], 3).unwrap()
}

/// One note from silence: earlier notes are cut so renders compare.
fn render(processor: &mut Rf7Processor, blocks: usize) -> Vec<f32> {
    processor.reset();
    let mut collected = Vec::new();
    for block in 0..blocks {
        let mut output = vec![0.0; FRAMES as usize * 2];
        let events = if block == 0 {
            vec![note_on(60)]
        } else {
            Vec::new()
        };
        processor.process(&[], &mut output, &events, &[], FRAMES, 0, 2);
        collected.extend_from_slice(&output);
    }
    collected
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
}

fn fields<'a>(
    page: &'a ProgramEditorPage,
    out: &mut Vec<&'a rackforge_program_api::ProgramEditorField>,
) {
    out.extend(page.fields.iter());
    for child in &page.pages {
        fields(child, out);
    }
}

#[test]
fn the_plugin_declares_a_declarative_editor_with_preview() {
    let processor = Rf7Processor::default();
    let capabilities = processor.program_editing_capabilities();
    assert_eq!(
        capabilities,
        rackforge_plugin_sdk::PROGRAM_EDIT_BASIC
            | rackforge_plugin_sdk::PROGRAM_EDIT_PREVIEW
            | rackforge_plugin_sdk::PROGRAM_EDIT_DECLARATIVE
    );
}

#[test]
fn a_library_voice_opens_as_a_copy_and_the_view_covers_the_whole_voice() {
    let mut processor = prepared();
    let opened = begin(&mut processor, Some("program-004"));
    assert_eq!(opened.document.name, "RF BRASS");
    assert_eq!(opened.document.id, "user.rf7-001", "a copy gets a new id");
    assert_eq!(opened.preview_sound_id, "custom.user.rf7-001");
    assert_eq!(opened.document.plugin_id, "org.rackforge.rf7");
    assert!(
        opened.artifacts.len() >= 2,
        "the voice's own dump and the bank exports"
    );
    assert!(opened.artifacts[0].storage_path.ends_with(".syx"));
    let dump = decode_voice_dump(&opened.artifacts[0].bytes).expect("a valid voice dump");
    assert_eq!(dump.voice, factory_voice(3));

    let view = view(&mut processor, &opened.document);
    assert_eq!(view.title, "RF BRASS");
    let labels: Vec<&str> = view.pages.iter().map(|page| page.label.as_str()).collect();
    assert_eq!(labels, ["Voice", "LFO", "Pitch envelope", "Operators"]);
    assert_eq!(view.pages[3].pages.len(), 6);
    assert_eq!(
        view.pages[3].pages[0].pages.len(),
        4,
        "four groups per operator"
    );
    let mut all = Vec::new();
    for page in &view.pages {
        fields(page, &mut all);
    }
    // 5 voice + 6 LFO + 8 pitch EG + 6 × 21 operator fields.
    assert_eq!(all.len(), 5 + 6 + 8 + 6 * 21);
    assert!(all.iter().all(|field| field.live_preview));
    let algorithm = all.iter().find(|field| field.id == "algorithm").unwrap();
    match &algorithm.kind {
        ProgramEditorFieldKind::Choice { options } => assert_eq!(options.len(), 32),
        other => panic!("algorithm is {other:?}"),
    }
}

#[test]
fn an_edit_previews_saves_installs_and_plays_from_the_catalog() {
    let mut processor = prepared();
    let opened = begin(&mut processor, Some("program-001"));
    let quiet = apply(
        &mut processor,
        &opened.document,
        "op1.out",
        ProgramEditorValue::Integer(0),
    )
    .expect("a valid edit");
    let silent = apply(
        &mut processor,
        &quiet.document,
        "op3.out",
        ProgramEditorValue::Integer(0),
    )
    .expect("a valid edit");
    // RF TINES is algorithm 5: carriers 1, 3 and 5. Silence two of them and
    // the preview is quieter than the original, without touching the library.
    let original = peak(&render(&mut processor, 20));
    assert!(preview(&mut processor, &silent));
    let previewed = peak(&render(&mut processor, 20));
    assert!(
        previewed > 0.0 && previewed < original * 0.6,
        "{previewed} vs {original}"
    );
    assert!(processor.load_preset("program-001"));
    assert!(
        (peak(&render(&mut processor, 20)) - original).abs() < 1e-6,
        "the library is untouched"
    );

    // A wrong value or field is refused, not clamped.
    assert!(
        apply(
            &mut processor,
            &silent.document,
            "op1.out",
            ProgramEditorValue::Integer(100)
        )
        .is_none()
    );
    assert!(
        apply(
            &mut processor,
            &silent.document,
            "op9.out",
            ProgramEditorValue::Integer(1)
        )
        .is_none()
    );
    assert!(
        apply(
            &mut processor,
            &silent.document,
            "op1.out",
            ProgramEditorValue::Inherited
        )
        .is_none()
    );

    // Save under a name of the user's choosing.
    let mut document = silent.document.clone();
    document.name = "Quiet tines for the bridge".into();
    let saved = save(&mut processor, &document);
    assert_eq!(saved.storage_path, "programs/user-rf7-001.json");
    let catalog = catalog(&mut processor);
    let entry = catalog
        .presets
        .iter()
        .find(|preset| preset.id == "custom.user.rf7-001")
        .expect("the saved program is in the catalog");
    assert_eq!(entry.name, "Quiet tines for the bridge");
    assert!(entry.editable);
    assert_eq!(entry.bank.as_deref(), Some("bank-user"));
    assert!(
        catalog.presets.iter().all(|p| p.editable),
        "every sound opens in the editor: a library voice as a copy"
    );

    // It plays from the catalog and is what was previewed.
    assert!(processor.load_preset("custom.user.rf7-001"));
    assert_eq!(processor.selected_sound_id(), "custom.user.rf7-001");
    assert!((peak(&render(&mut processor, 20)) - previewed).abs() < 1e-6);
    // The exported dump carries the new name, ten bytes of it.
    let dump = decode_voice_dump(&saved.artifacts[0].bytes).unwrap();
    assert_eq!(&dump.voice.name, b"Quiet tine");
    assert_eq!(dump.voice.operators[0].output_level, 0);
    assert!(
        !processor.load_preset("custom.user.rf7-002"),
        "not saved, not selectable"
    );
}

#[test]
fn a_saved_program_reopens_under_its_own_id_and_a_new_one_starts_from_init() {
    let mut processor = prepared();
    let opened = begin(&mut processor, Some("program-002"));
    save(&mut processor, &opened.document);
    let reopened = begin(&mut processor, Some("custom.user.rf7-001"));
    assert_eq!(
        reopened.document.id, "user.rf7-001",
        "the same program, not a copy"
    );
    let fresh = begin(&mut processor, None);
    assert_eq!(fresh.document.id, "user.rf7-002");
    assert_eq!(fresh.document.name, "RF NEW");
    let another_copy = begin(&mut processor, Some("program-002"));
    assert_eq!(
        another_copy.document.id, "user.rf7-002",
        "ids are taken only by saving"
    );
    let source =
        serde_json::to_vec(&ProgramEditRequest::new(Some("custom.nothing".into()))).unwrap();
    assert_eq!(
        processor.begin_program_edit(&source, &mut [0u8; 1024]),
        None
    );
    let source = serde_json::to_vec(&ProgramEditRequest::new(Some("program-099".into()))).unwrap();
    assert_eq!(
        processor.begin_program_edit(&source, &mut [0u8; 1024]),
        None
    );
}

#[test]
fn every_save_leaves_the_saved_programs_and_the_factory_bank_as_bulk_dumps() {
    let mut processor = prepared();
    let opened = begin(&mut processor, Some("program-001"));
    let saved = save(&mut processor, &opened.document);
    let paths: Vec<&str> = saved
        .artifacts
        .iter()
        .map(|a| a.storage_path.as_str())
        .collect();
    // The single voice, one bank of saved programs, and the factory
    // library's thirty-two and six.
    assert_eq!(
        paths,
        [
            "programs/user-rf7-001.syx",
            "exports/rf7-programs-1.syx",
            "exports/rf7-factory-1.syx",
            "exports/rf7-factory-2.syx",
        ]
    );
    let programs = decode_library(&saved.artifacts[1].bytes).expect("a valid bulk dump");
    assert_eq!(programs.len(), VOICES_PER_CARTRIDGE);
    assert_eq!(
        rf7_voice::printable_name(&programs.voice(0).unwrap().name).trim(),
        "RF TINES",
        "the program just saved is the first voice of the bank"
    );
    assert_eq!(
        rf7_voice::printable_name(&programs.voice(1).unwrap().name).trim(),
        "INIT VOICE",
        "and the rest of the bank is padding"
    );
    let factory = decode_library(&saved.artifacts[2].bytes).unwrap();
    assert_eq!(factory.voice(0), Some(&factory_voice(0)));
    let second = decode_library(&saved.artifacts[3].bytes).unwrap();
    assert_eq!(second.voice(5), Some(&factory_voice(37)));
    assert_eq!(
        rf7_voice::printable_name(&second.voice(6).unwrap().name).trim(),
        "INIT VOICE"
    );

    // A second save keeps the first program in its slot and adds the next;
    // with a cartridge installed the factory banks are not written.
    let mut bank = Vec::new();
    for _ in 0..VOICES_PER_CARTRIDGE {
        bank.extend_from_slice(&encode_packed(&factory_voice(9)));
    }
    assert!(processor.begin_resource(RESOURCE_CARTRIDGE, bank.len() as u64));
    assert!(processor.write_resource(0, &bank));
    assert!(processor.end_resource());
    let opened = begin(&mut processor, None);
    let saved = save(&mut processor, &opened.document);
    let paths: Vec<&str> = saved
        .artifacts
        .iter()
        .map(|a| a.storage_path.as_str())
        .collect();
    assert_eq!(
        paths,
        ["programs/user-rf7-002.syx", "exports/rf7-programs-1.syx"]
    );
    let programs = decode_library(&saved.artifacts[1].bytes).unwrap();
    assert_eq!(
        rf7_voice::printable_name(&programs.voice(1).unwrap().name).trim(),
        "RF NEW"
    );
}

#[test]
fn a_program_change_past_the_library_reaches_the_saved_programs() {
    let mut processor = prepared();
    let opened = begin(&mut processor, Some("program-001"));
    save(&mut processor, &opened.document);
    let library = processor.program_count();
    let change = |processor: &mut Rf7Processor, number: u8| {
        let event = [MidiEvent::new(0, [0xc0, number, 0], 2).unwrap()];
        let mut output = vec![0.0; FRAMES as usize * 2];
        processor.process(&[], &mut output, &event, &[], FRAMES, 0, 2);
    };
    change(&mut processor, library as u8);
    assert_eq!(processor.selected_sound_id(), "custom.user.rf7-001");
    // A number nothing answers to leaves the selection alone.
    change(&mut processor, library as u8 + 1);
    assert_eq!(processor.selected_sound_id(), "custom.user.rf7-001");
    change(&mut processor, 4);
    assert_eq!(processor.selected_sound_id(), "program-005");
}

#[test]
fn saved_programs_survive_a_new_cartridge_a_reload_and_a_session() {
    let mut processor = prepared();
    let opened = begin(&mut processor, Some("program-001"));
    let saved = save(&mut processor, &opened.document);
    assert!(processor.load_preset("custom.user.rf7-001"));
    let before = peak(&render(&mut processor, 20));

    // A cartridge replaces the library and leaves the saved program playing.
    let mut bank = Vec::new();
    for _ in 0..VOICES_PER_CARTRIDGE {
        bank.extend_from_slice(&encode_packed(&factory_voice(9)));
    }
    assert!(processor.begin_resource(RESOURCE_CARTRIDGE, bank.len() as u64));
    assert!(processor.write_resource(0, &bank));
    assert!(processor.end_resource());
    assert_eq!(processor.selected_sound_id(), "custom.user.rf7-001");
    assert!((peak(&render(&mut processor, 20)) - before).abs() < 1e-6);
    assert_eq!(
        catalog(&mut processor).presets.len(),
        VOICES_PER_CARTRIDGE + 1
    );

    // The session remembers the selection; a fresh instance gets the saved
    // program back the way the host does it — prepare, then install — and
    // only then reads the state.
    let mut state = [0u8; 1024];
    let length = processor.save_state(&mut state).unwrap();
    let mut reloaded = prepared();
    assert!(reloaded.load_state(&state[..length]), "the state opens");
    assert_eq!(
        reloaded.selected_sound_id(),
        "program-001",
        "without the program installed, the library slot plays"
    );
    let mut reloaded = prepared();
    let reprepared = {
        let source = serde_json::to_vec(&saved.document).unwrap();
        let mut buffer = vec![0u8; TRANSFER_BYTES];
        let length = reloaded.prepare_program_save(&source, &mut buffer).unwrap();
        buffer.truncate(length);
        buffer
    };
    assert!(reloaded.install_program(&reprepared));
    assert!(reloaded.load_state(&state[..length]));
    assert_eq!(reloaded.selected_sound_id(), "custom.user.rf7-001");
    assert!((peak(&render(&mut reloaded, 20)) - before).abs() < 1e-6);

    // A state that names no saved program is still the two-version form, so
    // an older RF-7 opens it.
    assert!(reloaded.load_preset("program-001"));
    let length = reloaded.save_state(&mut state).unwrap();
    assert_eq!(u32::from_le_bytes(state[4..8].try_into().unwrap()), 2);
    assert!(reloaded.load_preset("custom.user.rf7-001"));
    let length_with_custom = reloaded.save_state(&mut state).unwrap();
    assert_eq!(u32::from_le_bytes(state[4..8].try_into().unwrap()), 3);
    assert_eq!(length_with_custom, length + 1 + "user.rf7-001".len());
    // A damaged id is refused whole.
    let mut broken = state[..length_with_custom].to_vec();
    broken[length] = 3;
    assert!(!reloaded.load_state(&broken));
    let mut shouting = state[..length_with_custom].to_vec();
    shouting[length + 1] = b'U';
    assert!(!reloaded.load_state(&shouting));
}

#[test]
fn a_document_from_another_plugin_or_another_time_is_refused() {
    let mut processor = prepared();
    let opened = begin(&mut processor, Some("program-001"));
    let mut foreign = opened.document.clone();
    foreign.plugin_id = "org.example.other".into();
    let source = serde_json::to_vec(&foreign).unwrap();
    assert_eq!(
        processor.prepare_program_save(&source, &mut [0u8; 4096]),
        None
    );
    assert_eq!(
        processor.program_editor_view(&source, &mut [0u8; 4096]),
        None
    );
    let mut future = opened.document.clone();
    future.payload_version = 2;
    let source = serde_json::to_vec(&future).unwrap();
    assert_eq!(
        processor.prepare_program_save(&source, &mut [0u8; 4096]),
        None
    );
    assert!(!processor.install_program(b"not json"));
    assert!(!processor.preview_program(b"{}"));
    // A buffer too small for the answer gets nothing, not a truncated answer.
    let source = serde_json::to_vec(&opened.document).unwrap();
    assert_eq!(
        processor.program_editor_view(&source, &mut [0u8; 100]),
        None
    );
}
