//! The program catalog RackForge asks for after a library is delivered or a
//! program is saved.
//!
//! Written by hand into the host's transfer buffer. A serializer would be
//! easier to read, but this runs inside the guest with a fixed buffer and no
//! room to fail halfway: either the whole catalog fits or nothing is written.
//!
//! The library's voices come first, in banks of thirty-two. The programs the
//! user saved from the editor follow in a bank of their own. Every entry is
//! marked editable, because the host offers its editor only for sounds that
//! say so: a library voice opens as a copy, a saved program opens in place.

use crate::CARTRIDGE_BAYS;
use crate::programs::{CustomPrograms, PREFIX as CUSTOM_PREFIX};
use rf7_voice::{FACTORY_VOICES, Library, MAX_VOICES, printable_name};

const PREFIX: &str = "program-";
const BANK_PREFIX: &str = "bank-";
const USER_BANK: &str = "bank-user";
/// The one bank there is while no bay holds a cartridge.
pub const FACTORY_BANK: &str = "bank-factory";

/// Turn a catalog identifier back into a slot. Anything else is not ours.
pub fn program_index(id: &str) -> Option<usize> {
    let digits = id.strip_prefix(PREFIX)?;
    if digits.len() != 3 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number: usize = digits.parse().ok()?;
    // Lazily: slot zero has no program number, and 0 - 1 would wrap.
    (1..=MAX_VOICES).contains(&number).then(|| number - 1)
}

/// The bank a program belongs to: the factory's, at the head of the library,
/// or the bay whose stretch of it the program falls in.
fn bank_of(slot: usize, bays: &[usize]) -> Option<usize> {
    let mut first = FACTORY_VOICES;
    if slot < first {
        return None;
    }
    for (bay, length) in bays.iter().enumerate() {
        if slot < first + length {
            return Some(bay);
        }
        first += length;
    }
    None
}

/// Write the whole catalog, or write nothing and return `None`.
///
/// The first bank is the factory's, which is always there. After it comes a
/// bank per filled bay, under that bay's name, which is what lets the
/// surfaces show a rack rather than one long list.
pub fn write(
    library: &Library,
    bays: &[usize],
    custom: &CustomPrograms,
    destination: &mut [u8],
) -> Option<usize> {
    let mut out = Writer {
        destination,
        written: 0,
        overflowed: false,
    };
    out.text("{\"schema_version\":1,\"banks\":[{\"id\":\"");
    out.text(FACTORY_BANK);
    out.text("\",\"name\":\"Factory bank\",\"order\":0}");
    for (bay, length) in bays.iter().enumerate() {
        if *length == 0 {
            continue;
        }
        out.text(",{\"id\":\"");
        out.bank_id(bay);
        out.text("\",\"name\":\"Cartridge ");
        out.number(bay + 1);
        out.text("\",\"order\":");
        out.number(bay + 1);
        out.text("}");
    }
    if !custom.is_empty() {
        out.text(",{\"id\":\"");
        out.text(USER_BANK);
        out.text("\",\"name\":\"Your programs\",\"order\":");
        out.number(CARTRIDGE_BAYS + 1);
        out.text("}");
    }
    out.text("],\"presets\":[");
    for slot in 0..library.len().max(1) {
        if slot > 0 {
            out.text(",");
        }
        out.text("{\"id\":\"");
        out.text(PREFIX);
        out.three_digits(slot + 1);
        out.text("\",\"name\":\"");
        let name = library
            .voice(slot)
            .map_or("", |voice| printable_name(&voice.name));
        if name.is_empty() {
            // A blank slot still needs a name a user can point at.
            out.text("PROGRAM ");
            out.three_digits(slot + 1);
        } else {
            out.escaped(name);
        }
        out.text("\",\"bank\":\"");
        match bank_of(slot, bays) {
            Some(bay) => out.bank_id(bay),
            None => out.text(FACTORY_BANK),
        }
        out.text("\",\"category\":\"FM\",\"editable\":true,\"order\":");
        out.number(slot);
        out.text("}");
    }
    for (index, program) in custom.entries().iter().enumerate() {
        out.text(",{\"id\":\"");
        out.text(CUSTOM_PREFIX);
        out.text(&program.id);
        out.text("\",\"name\":\"");
        out.escaped(&program.name);
        out.text("\",\"bank\":\"");
        out.text(USER_BANK);
        out.text("\",\"category\":\"FM\",\"editable\":true,\"order\":");
        out.number(library.len().max(1) + index);
        out.text("}");
    }
    out.text("]}");
    out.finish()
}

struct Writer<'a> {
    destination: &'a mut [u8],
    written: usize,
    overflowed: bool,
}

impl Writer<'_> {
    fn byte(&mut self, byte: u8) {
        match self.destination.get_mut(self.written) {
            Some(slot) => {
                *slot = byte;
                self.written += 1;
            }
            None => self.overflowed = true,
        }
    }

    fn text(&mut self, text: &str) {
        for byte in text.bytes() {
            self.byte(byte);
        }
    }

    fn bank_id(&mut self, bank: usize) {
        self.text(BANK_PREFIX);
        self.number(bank + 1);
    }

    /// Voice names are printable ASCII, but nothing stops one holding a
    /// quote; a name typed into the host can hold anything at all.
    fn escaped(&mut self, text: &str) {
        for byte in text.bytes() {
            match byte {
                b'"' | b'\\' => {
                    self.byte(b'\\');
                    self.byte(byte);
                }
                0x20..=0x7e => self.byte(byte),
                0x80.. => self.byte(byte),
                _ => self.byte(b' '),
            }
        }
    }

    fn three_digits(&mut self, value: usize) {
        self.byte(b'0' + (value / 100 % 10) as u8);
        self.byte(b'0' + (value / 10 % 10) as u8);
        self.byte(b'0' + (value % 10) as u8);
    }

    fn number(&mut self, value: usize) {
        if value >= 10 {
            self.number(value / 10);
        }
        self.byte(b'0' + (value % 10) as u8);
    }

    fn finish(self) -> Option<usize> {
        (!self.overflowed).then_some(self.written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::{FACTORY_VOICES, Voice, factory_library, factory_voice};

    /// The factory bank, which is what plays while every bay is empty.
    const NO_BAYS: [usize; CARTRIDGE_BAYS] = [0; CARTRIDGE_BAYS];

    fn rendered(library: &Library) -> String {
        rendered_with(library, &CustomPrograms::default())
    }

    fn rendered_with(library: &Library, custom: &CustomPrograms) -> String {
        rendered_in(library, &NO_BAYS, custom)
    }

    fn rendered_in(library: &Library, bays: &[usize], custom: &CustomPrograms) -> String {
        let mut buffer = vec![0u8; crate::TRANSFER_BYTES];
        let length = write(library, bays, custom, &mut buffer).expect("the catalog must fit");
        core::str::from_utf8(&buffer[..length])
            .expect("ASCII only")
            .to_owned()
    }

    #[test]
    fn the_catalog_lists_exactly_the_voices_the_library_holds() {
        let json = rendered(&factory_library());
        assert!(json.starts_with("{\"schema_version\":1,"));
        assert!(json.ends_with("]}"));
        assert_eq!(json.matches("\"id\":\"program-").count(), FACTORY_VOICES);
        assert!(json.contains("\"id\":\"program-001\""));
        assert!(json.contains("\"name\":\"RF TINES\""));
        assert!(json.contains("\"id\":\"program-038\""));
        assert!(
            !json.contains("program-039"),
            "no padding past the second bank"
        );
        // One bank, because no bay holds a cartridge: the factory's.
        assert_eq!(json.matches("\"id\":\"bank-").count(), 1);
    }

    #[test]
    fn the_largest_library_still_fits_the_transfer_buffer() {
        let mut voice = factory_voice(0);
        voice.name = *b"\\\"\\\"\\\"\\\"\\\"";
        let library = Library::from_voices(&[voice; MAX_VOICES]);
        let mut custom = CustomPrograms::default();
        for number in 0..crate::programs::MAX_CUSTOM_PROGRAMS {
            assert!(custom.install(crate::programs::CustomProgram {
                id: format!("user.rf7-{number:03}"),
                name: "\"".repeat(64),
                voice,
            }));
        }
        let mut buffer = vec![0u8; crate::TRANSFER_BYTES];
        let length =
            write(&library, &NO_BAYS, &custom, &mut buffer).expect("256 escaped names must fit");
        assert!(length <= crate::TRANSFER_BYTES);
        assert!(serde_json::from_slice::<serde_json::Value>(&buffer[..length]).is_ok());
    }

    #[test]
    fn saved_programs_follow_the_library_in_their_own_bank() {
        let mut custom = CustomPrograms::default();
        assert!(custom.install(crate::programs::CustomProgram {
            id: "user.rf7-001".into(),
            name: "My \"tines\" \u{e9}".into(),
            voice: factory_voice(0),
        }));
        let json = rendered_with(&factory_library(), &custom);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let presets = value["presets"].as_array().unwrap();
        assert_eq!(presets.len(), FACTORY_VOICES + 1);
        let last = presets.last().unwrap();
        assert_eq!(last["id"], "custom.user.rf7-001");
        assert_eq!(last["name"], "My \"tines\" \u{e9}");
        assert_eq!(last["bank"], "bank-user");
        assert_eq!(last["editable"], true);
        assert_eq!(last["order"], FACTORY_VOICES);
        assert_eq!(
            presets[0]["editable"], true,
            "a library voice opens in the editor, as a copy"
        );
        let banks = value["banks"].as_array().unwrap();
        assert_eq!(banks.len(), 2, "the factory bank, and yours behind it");
        assert_eq!(banks[1]["id"], "bank-user");
        assert!(
            !rendered(&factory_library()).contains("bank-user"),
            "no empty bank"
        );
    }

    #[test]
    fn a_bank_is_a_bay_behind_the_factory_bank_that_is_always_there() {
        // The factory bank, then two bays filled out of eight.
        let voices: Vec<Voice> = (0..FACTORY_VOICES + 40)
            .map(|n| factory_voice(n % FACTORY_VOICES))
            .collect();
        let library = Library::from_voices(&voices);
        let mut bays = NO_BAYS;
        bays[0] = 32;
        bays[2] = 8;
        let json = rendered_in(&library, &bays, &CustomPrograms::default());
        assert!(json.contains("{\"id\":\"bank-factory\",\"name\":\"Factory bank\",\"order\":0}"));
        assert!(json.contains("{\"id\":\"bank-1\",\"name\":\"Cartridge 1\",\"order\":1}"));
        assert!(json.contains("{\"id\":\"bank-3\",\"name\":\"Cartridge 3\",\"order\":3}"));
        assert!(!json.contains("bank-2"), "an empty bay is not a bank");
        // The last program of bay one, and the first of bay three.
        let bank_of = |program: &str| {
            let at = json.find(&format!("\"id\":\"{program}\"")).expect(program);
            let entry = &json[at..(at + 120).min(json.len())];
            let at = entry.find("\"bank\":\"").expect("a bank") + 8;
            entry[at..].split('"').next().expect("a name").to_owned()
        };
        // The factory's last program, then bay one's, then bay three's.
        assert_eq!(
            bank_of(&format!("program-{:03}", FACTORY_VOICES)),
            "bank-factory"
        );
        assert_eq!(
            bank_of(&format!("program-{:03}", FACTORY_VOICES + 1)),
            "bank-1"
        );
        assert_eq!(
            bank_of(&format!("program-{:03}", FACTORY_VOICES + 32)),
            "bank-1"
        );
        assert_eq!(
            bank_of(&format!("program-{:03}", FACTORY_VOICES + 33)),
            "bank-3"
        );

        // With every bay empty there is one bank, and it is the factory's.
        let json = rendered(&factory_library());
        assert!(json.contains("{\"id\":\"bank-factory\",\"name\":\"Factory bank\",\"order\":0}"));
        assert!(json.contains("\"bank\":\"bank-factory\""));
        assert!(!json.contains("bank-1"));
    }

    #[test]
    fn a_buffer_too_small_writes_nothing_rather_than_half_a_catalog() {
        for capacity in [0, 1, 64, 200] {
            let mut buffer = vec![0u8; capacity];
            assert_eq!(
                write(
                    &factory_library(),
                    &NO_BAYS,
                    &CustomPrograms::default(),
                    &mut buffer
                ),
                None
            );
        }
    }

    #[test]
    fn a_name_that_could_break_the_json_is_escaped() {
        let mut voice = factory_voice(0);
        voice.name = *br#"SAY "HI"\ "#;
        let json = rendered(&Library::from_voices(&[voice]));
        assert!(json.contains(r#""name":"SAY \"HI\"\\""#), "{json}");
    }

    #[test]
    fn a_blank_slot_still_gets_a_name() {
        let mut voice = Voice::init();
        voice.name = *b"          ";
        let json = rendered(&Library::from_voices(&[voice; 2]));
        assert!(json.contains("\"name\":\"PROGRAM 001\""));
        assert!(json.contains("\"name\":\"PROGRAM 002\""));
    }

    #[test]
    fn only_our_own_identifiers_select_a_program() {
        assert_eq!(program_index("program-001"), Some(0));
        assert_eq!(program_index("program-032"), Some(31));
        assert_eq!(program_index("program-320"), Some(319));
        assert_eq!(program_index("program-000"), None);
        assert_eq!(program_index("program-321"), None);
        assert_eq!(program_index("program-01"), None, "the old two-digit form");
        assert_eq!(program_index("program-0001"), None);
        assert_eq!(program_index("program-00x"), None);
        assert_eq!(program_index("research-direct"), None);
        assert_eq!(program_index(""), None);
    }
}
