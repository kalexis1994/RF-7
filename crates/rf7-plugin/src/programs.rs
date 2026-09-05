//! The programs a user has saved from the editor.
//!
//! They live beside the library, not in it: a cartridge the user installs
//! later replaces the library's thirty-two voices and leaves these alone. The
//! host owns their files and re-installs each one when the instrument loads,
//! so nothing here is persisted by the plugin itself.

use rf7_voice::Voice;

/// The catalog prefix the host expects on a saved program.
pub const PREFIX: &str = "custom.";
/// As many as the library may hold; a user who saves more is asked to make
/// room, not silently trimmed.
pub const MAX_CUSTOM_PROGRAMS: usize = 128;
const ID_PREFIX: &str = "user.rf7-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomProgram {
    /// The document identifier, without the catalog prefix.
    pub id: String,
    /// The name the user gave it, which can be longer than the ten bytes a
    /// voice carries.
    pub name: String,
    pub voice: Voice,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomPrograms {
    entries: Vec<CustomProgram>,
}

impl CustomPrograms {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[CustomProgram] {
        &self.entries
    }

    /// The document id behind a catalog id, if it is one of ours.
    pub fn document_id(catalog_id: &str) -> Option<&str> {
        catalog_id.strip_prefix(PREFIX).filter(|id| !id.is_empty())
    }

    pub fn find(&self, id: &str) -> Option<&CustomProgram> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Install or replace. A saved program keeps its place in the list when
    /// it is saved again, so the catalog does not reorder under the user.
    pub fn install(&mut self, program: CustomProgram) -> bool {
        if let Some(slot) = self.entries.iter_mut().find(|entry| entry.id == program.id) {
            *slot = program;
            return true;
        }
        if self.entries.len() >= MAX_CUSTOM_PROGRAMS {
            return false;
        }
        self.entries.push(program);
        true
    }

    /// The first identifier not yet taken: `user.rf7-001`, `user.rf7-002`...
    pub fn next_id(&self) -> String {
        (1..=MAX_CUSTOM_PROGRAMS + 1)
            .map(|number| format!("{ID_PREFIX}{number:03}"))
            .find(|candidate| self.find(candidate).is_none())
            .expect("more candidates than programs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::factory_voice;

    fn program(id: &str) -> CustomProgram {
        CustomProgram {
            id: id.to_owned(),
            name: id.to_uppercase(),
            voice: factory_voice(0),
        }
    }

    #[test]
    fn saving_again_keeps_the_place_and_new_ones_go_to_the_end() {
        let mut programs = CustomPrograms::default();
        assert_eq!(programs.next_id(), "user.rf7-001");
        assert!(programs.install(program("user.rf7-001")));
        assert_eq!(programs.next_id(), "user.rf7-002");
        assert!(programs.install(program("mine")));
        let mut renamed = program("user.rf7-001");
        renamed.name = "FIRST".into();
        assert!(programs.install(renamed));
        assert_eq!(programs.len(), 2);
        assert_eq!(programs.entries()[0].name, "FIRST");
        assert_eq!(programs.entries()[1].id, "mine");
        assert_eq!(programs.find("mine").map(|p| p.name.as_str()), Some("MINE"));
        assert_eq!(programs.find("nothing"), None);
    }

    #[test]
    fn the_catalog_prefix_is_required_and_the_rest_is_the_document_id() {
        assert_eq!(
            CustomPrograms::document_id("custom.user.rf7-001"),
            Some("user.rf7-001")
        );
        assert_eq!(CustomPrograms::document_id("custom."), None);
        assert_eq!(CustomPrograms::document_id("program-001"), None);
    }

    #[test]
    fn the_store_is_bounded() {
        let mut programs = CustomPrograms::default();
        for number in 0..MAX_CUSTOM_PROGRAMS {
            assert!(programs.install(program(&format!("p{number}"))));
        }
        assert!(!programs.install(program("one-more")));
        assert!(
            programs.install(program("p3")),
            "replacing is always allowed"
        );
        assert_eq!(programs.len(), MAX_CUSTOM_PROGRAMS);
    }
}
