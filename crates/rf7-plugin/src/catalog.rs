//! The program catalog RackForge asks for after a cartridge is delivered.
//!
//! Written by hand into the host's transfer buffer. A serializer would be
//! easier to read, but this runs inside the guest with a fixed buffer and no
//! room to fail halfway: either the whole catalog fits or nothing is written.

use rf7_voice::{Cartridge, VOICES_PER_CARTRIDGE, printable_name};

const BANK: &str = "cartridge";
const PREFIX: &str = "program-";

/// Turn a catalog id back into a slot. Anything else is not ours.
pub fn program_index(id: &str) -> Option<usize> {
    let digits = id.strip_prefix(PREFIX)?;
    if digits.len() != 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number: usize = digits.parse().ok()?;
    // Lazily: slot zero has no program number, and 0 - 1 would wrap.
    (1..=VOICES_PER_CARTRIDGE)
        .contains(&number)
        .then(|| number - 1)
}

/// Write the whole catalog, or write nothing and return `None`.
pub fn write(cartridge: &Cartridge, destination: &mut [u8]) -> Option<usize> {
    let mut out = Writer {
        destination,
        written: 0,
        overflowed: false,
    };
    out.text("{\"schema_version\":1,\"banks\":[{\"id\":\"");
    out.text(BANK);
    out.text("\",\"name\":\"Cartridge\",\"order\":0}],\"presets\":[");
    for slot in 0..VOICES_PER_CARTRIDGE {
        if slot > 0 {
            out.text(",");
        }
        out.text("{\"id\":\"");
        out.text(PREFIX);
        out.two_digits(slot + 1);
        out.text("\",\"name\":\"");
        let name = cartridge
            .voice(slot)
            .map_or("", |voice| printable_name(&voice.name));
        if name.is_empty() {
            // A blank slot still needs a name a user can point at.
            out.text("PROGRAM ");
            out.two_digits(slot + 1);
        } else {
            out.escaped(name);
        }
        out.text("\",\"bank\":\"");
        out.text(BANK);
        out.text("\",\"category\":\"FM\",\"order\":");
        out.number(slot);
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

    /// Voice names are printable ASCII, but nothing stops one holding a quote.
    fn escaped(&mut self, text: &str) {
        for byte in text.bytes() {
            if byte == b'"' || byte == b'\\' {
                self.byte(b'\\');
            }
            self.byte(byte);
        }
    }

    fn two_digits(&mut self, value: usize) {
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
    use rf7_voice::{Voice, factory_cartridge, factory_voice};

    #[test]
    fn the_catalog_lists_every_slot_of_the_loaded_cartridge() {
        let mut buffer = [0u8; crate::TRANSFER_BYTES];
        let length = write(&factory_cartridge(), &mut buffer).expect("the catalog must fit");
        let json = core::str::from_utf8(&buffer[..length]).expect("ASCII only");
        assert!(json.starts_with("{\"schema_version\":1,"));
        assert!(json.ends_with("]}"));
        assert_eq!(
            json.matches("\"id\":\"program-").count(),
            VOICES_PER_CARTRIDGE
        );
        assert!(json.contains("\"name\":\"RF TINES\""));
        assert!(json.contains("\"id\":\"program-32\""));
        assert!(json.contains("\"order\":31"));
    }

    #[test]
    fn a_buffer_too_small_writes_nothing_rather_than_half_a_catalog() {
        for capacity in [0, 1, 64, 512] {
            let mut buffer = vec![0u8; capacity];
            assert_eq!(write(&factory_cartridge(), &mut buffer), None);
        }
    }

    #[test]
    fn a_name_that_could_break_the_json_is_escaped() {
        let mut voice = factory_voice(0);
        voice.name = *br#"SAY "HI"\ "#;
        let mut voices = [Voice::init(); VOICES_PER_CARTRIDGE];
        voices[0] = voice;
        let mut buffer = [0u8; crate::TRANSFER_BYTES];
        let length = write(&Cartridge::from_voices(voices), &mut buffer).unwrap();
        let json = core::str::from_utf8(&buffer[..length]).unwrap();
        assert!(json.contains(r#""name":"SAY \"HI\"\\""#), "{json}");
    }

    #[test]
    fn a_blank_slot_still_gets_a_name() {
        let mut voice = Voice::init();
        voice.name = *b"          ";
        let mut buffer = [0u8; crate::TRANSFER_BYTES];
        let length = write(
            &Cartridge::from_voices([voice; VOICES_PER_CARTRIDGE]),
            &mut buffer,
        )
        .unwrap();
        let json = core::str::from_utf8(&buffer[..length]).unwrap();
        assert!(json.contains("\"name\":\"PROGRAM 01\""));
        assert!(json.contains("\"name\":\"PROGRAM 32\""));
    }

    #[test]
    fn only_our_own_identifiers_select_a_program() {
        assert_eq!(program_index("program-01"), Some(0));
        assert_eq!(program_index("program-32"), Some(31));
        assert_eq!(program_index("program-00"), None);
        assert_eq!(program_index("program-33"), None);
        assert_eq!(program_index("program-1"), None);
        assert_eq!(program_index("program-001"), None);
        assert_eq!(program_index("program-0x"), None);
        assert_eq!(program_index("research-direct"), None);
        assert_eq!(program_index(""), None);
    }
}
