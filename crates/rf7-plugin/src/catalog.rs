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

use crate::programs::{CustomPrograms, PREFIX as CUSTOM_PREFIX};
use rf7_voice::{Library, MAX_VOICES, VOICES_PER_CARTRIDGE, printable_name};

const PREFIX: &str = "program-";
const BANK_PREFIX: &str = "bank-";
const USER_BANK: &str = "bank-user";

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

/// How many banks of thirty-two this library spans, at least one.
fn banks(library: &Library) -> usize {
    library.len().div_ceil(VOICES_PER_CARTRIDGE).max(1)
}

/// Write the whole catalog, or write nothing and return `None`.
pub fn write(library: &Library, custom: &CustomPrograms, destination: &mut [u8]) -> Option<usize> {
    let mut out = Writer {
        destination,
        written: 0,
        overflowed: false,
    };
    out.text("{\"schema_version\":1,\"banks\":[");
    for bank in 0..banks(library) {
        if bank > 0 {
            out.text(",");
        }
        out.text("{\"id\":\"");
        out.bank_id(bank);
        out.text("\",\"name\":\"Bank ");
        out.number(bank + 1);
        out.text("\",\"order\":");
        out.number(bank);
        out.text("}");
    }
    if !custom.is_empty() {
        out.text(",{\"id\":\"");
        out.text(USER_BANK);
        out.text("\",\"name\":\"Your programs\",\"order\":");
        out.number(banks(library));
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
        out.bank_id(slot / VOICES_PER_CARTRIDGE);
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

    fn rendered(library: &Library) -> String {
        rendered_with(library, &CustomPrograms::default())
    }

    fn rendered_with(library: &Library, custom: &CustomPrograms) -> String {
        let mut buffer = [0u8; crate::TRANSFER_BYTES];
        let length = write(library, custom, &mut buffer).expect("the catalog must fit");
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
        // A cartridge of thirty-two and a second bank of six.
        assert_eq!(json.matches("\"id\":\"bank-").count(), 2);
    }

    #[test]
    fn a_library_of_several_banks_is_grouped_by_thirty_two() {
        let library = Library::from_voices(&[factory_voice(0); 64]);
        let json = rendered(&library);
        assert_eq!(json.matches("\"id\":\"program-").count(), 64);
        assert_eq!(json.matches("\"id\":\"bank-").count(), 2);
        assert!(json.contains("\"name\":\"Bank 2\""));
        assert!(json.contains("\"id\":\"program-064\""));
        // The thirty-third voice is the first of the second bank.
        let thirty_third = json
            .split("\"id\":\"program-033\"")
            .nth(1)
            .expect("present");
        assert!(thirty_third.starts_with(",") || thirty_third.contains("bank-2"));
        assert!(thirty_third.split("}").next().unwrap().contains("bank-2"));
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
        let mut buffer = [0u8; crate::TRANSFER_BYTES];
        let length = write(&library, &custom, &mut buffer).expect("256 escaped names must fit");
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
        assert_eq!(banks.len(), 3);
        assert_eq!(banks[2]["id"], "bank-user");
        assert!(
            !rendered(&factory_library()).contains("bank-user"),
            "no empty bank"
        );
    }

    #[test]
    fn a_buffer_too_small_writes_nothing_rather_than_half_a_catalog() {
        for capacity in [0, 1, 64, 200] {
            let mut buffer = vec![0u8; capacity];
            assert_eq!(
                write(&factory_library(), &CustomPrograms::default(), &mut buffer),
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
        assert_eq!(program_index("program-128"), Some(127));
        assert_eq!(program_index("program-000"), None);
        assert_eq!(program_index("program-129"), None);
        assert_eq!(program_index("program-01"), None, "the old two-digit form");
        assert_eq!(program_index("program-0001"), None);
        assert_eq!(program_index("program-00x"), None);
        assert_eq!(program_index("research-direct"), None);
        assert_eq!(program_index(""), None);
    }
}
