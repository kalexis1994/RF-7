//! The cartridge, drawn.
//!
//! A DX7 voice cartridge is a near-square slab of dark plastic with a
//! coloured band along the spine — the band is what shows on a shelf — and
//! a dark label on its face: a large number, a hairline, two rows lettered A
//! and B naming the two groups of thirty-two the switch on the spine chose
//! between, a small print along one edge, and a serial in the corner. The
//! voice ROMs wore a turquoise band; the data RAMs a silver one lettered in
//! red. RF-7 draws its own cartridges the same way, in SVG, at whatever size
//! the shelf gives them: a saved-programs cartridge is the RAM, everything
//! else — the factory bank, a file — a ROM.
//!
//! Everything in the drawing is a parameter of [`Face`]. Text is escaped
//! and clipped to the label, and the gradient ids carry the card's own key,
//! so any number of cartridges can share one page.

use crate::render::esc;
use core::fmt::Write;

/// How many characters a label row keeps before it is cut with an ellipsis.
/// The label is 192 units wide and the row's type about 5.2 units a glyph.
const ROW_CHARS: usize = 27;
/// The serial in the corner is smaller, and shares its line with the print.
const SERIAL_CHARS: usize = 22;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rom,
    Ram,
}

/// What the label says.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// The large figure at the top of the label.
    pub number: String,
    pub kind: Kind,
    /// Rows A and B. An empty row B is printed faintly, as a one-bank
    /// cartridge's would be.
    pub rows: [String; 2],
    /// The serial in the corner: the file's name, or what stands for one.
    pub serial: String,
    pub voices: usize,
    pub banks: usize,
    /// A cartridge whose voices have not been read yet: printed in a
    /// quieter silver.
    pub dim: bool,
}

fn cut(text: &str, keep: usize) -> String {
    let count = text.chars().count();
    if count <= keep {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(keep.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A key safe inside an id attribute: the card's own, reduced to what an
/// id may hold.
fn id_key(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The cartridge as an SVG element. `key` tells this drawing's gradients
/// apart from the next one's.
pub fn svg(face: &Face, key: &str) -> String {
    let k = id_key(key);
    let rom = face.kind == Kind::Rom;
    let (spine_a, spine_b) = if rom {
        ("#33cfac", "#1c9f84")
    } else {
        ("#e6e3da", "#c4c0b3")
    };
    let ink = if rom { "#0a3a30" } else { "#26221f" };
    let (small, word, word_fill) = if rom {
        ("VOICE", "ROM", "#082a22")
    } else {
        ("DATA", "RAM", "#d9362b")
    };
    let silver = if face.dim { "#7c7a74" } else { "#d8d4c9" };
    let row_a = esc(&cut(&face.rows[0], ROW_CHARS));
    let row_b = esc(&cut(&face.rows[1], ROW_CHARS));
    let b_opacity = if face.rows[1].is_empty() { ".35" } else { "1" };
    let serial = esc(&cut(&face.serial, SERIAL_CHARS));
    let number = esc(&cut(&face.number, 4));
    let banks = match face.banks {
        1 => "1 BANK".to_owned(),
        n => format!("{n} BANKS"),
    };
    let mut out = String::with_capacity(4096);
    let _ = write!(
        out,
        "<svg class=\"cartridge\" viewBox=\"0 0 240 222\" xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" aria-label=\"{serial}\">\
<defs>\
<linearGradient id=\"cb-{k}\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"><stop offset=\"0\" stop-color=\"#3b3335\"/><stop offset=\".45\" stop-color=\"#241e1f\"/><stop offset=\"1\" stop-color=\"#151112\"/></linearGradient>\
<linearGradient id=\"cs-{k}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"{spine_a}\"/><stop offset=\"1\" stop-color=\"{spine_b}\"/></linearGradient>\
<linearGradient id=\"cl-{k}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"#26272c\"/><stop offset=\"1\" stop-color=\"#191a1e\"/></linearGradient>\
<linearGradient id=\"cg-{k}\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"><stop offset=\"0\" stop-color=\"#fff\" stop-opacity=\".10\"/><stop offset=\".5\" stop-color=\"#fff\" stop-opacity=\".02\"/><stop offset=\"1\" stop-color=\"#000\" stop-opacity=\".18\"/></linearGradient>\
<filter id=\"cf-{k}\" x=\"-10%\" y=\"-10%\" width=\"120%\" height=\"125%\"><feDropShadow dx=\"0\" dy=\"5\" stdDeviation=\"5\" flood-color=\"#000\" flood-opacity=\".55\"/></filter>\
<clipPath id=\"cc-{k}\"><rect x=\"24\" y=\"62\" width=\"192\" height=\"126\" rx=\"3\"/></clipPath>\
</defs>\
<g filter=\"url(#cf-{k})\">\
<rect x=\"26\" y=\"192\" width=\"188\" height=\"20\" rx=\"3\" fill=\"#0f0d0e\"/>\
<rect x=\"30\" y=\"196\" width=\"180\" height=\"2\" fill=\"#2a2426\" opacity=\".8\"/>\
<rect x=\"8\" y=\"8\" width=\"224\" height=\"192\" rx=\"10\" fill=\"url(#cb-{k})\"/>\
<rect x=\"9\" y=\"9\" width=\"222\" height=\"190\" rx=\"9.5\" fill=\"none\" stroke=\"#fff\" stroke-opacity=\".07\"/>\
<rect x=\"11\" y=\"11\" width=\"218\" height=\"186\" rx=\"8\" fill=\"none\" stroke=\"#000\" stroke-opacity=\".35\"/>\
</g>\
<rect x=\"18\" y=\"16\" width=\"204\" height=\"36\" rx=\"3\" fill=\"url(#cs-{k})\" stroke=\"{ink}\" stroke-opacity=\".45\" stroke-width=\".8\"/>\
<rect x=\"18.5\" y=\"16.5\" width=\"203\" height=\"35\" rx=\"2.6\" fill=\"none\" stroke=\"#fff\" stroke-opacity=\".28\"/>\
<g font-family=\"Arial Narrow, Roboto Condensed, Segoe UI, sans-serif\">\
<text x=\"30\" y=\"43\" font-size=\"23\" font-weight=\"900\" font-style=\"italic\" letter-spacing=\"-.5\" fill=\"{spine_a}\" fill-opacity=\".4\" stroke=\"{ink}\" stroke-width=\"1.5\" paint-order=\"stroke\">RF-7</text>\
<text x=\"112\" y=\"41.5\" font-size=\"8.5\" font-weight=\"700\" letter-spacing=\"1.2\" fill=\"{ink}\">{small}</text>\
<text x=\"212\" y=\"44\" text-anchor=\"end\" font-size=\"19\" font-weight=\"900\" letter-spacing=\"-.3\" fill=\"{word_fill}\">{word}</text>\
</g>\
<rect x=\"24\" y=\"62\" width=\"192\" height=\"126\" rx=\"3\" fill=\"url(#cl-{k})\" stroke=\"#3d3e44\" stroke-width=\".7\"/>\
<g clip-path=\"url(#cc-{k})\" font-family=\"Arial Narrow, Roboto Condensed, Segoe UI, sans-serif\" fill=\"{silver}\">\
<text x=\"120\" y=\"91\" text-anchor=\"middle\" font-size=\"26\" font-weight=\"700\" letter-spacing=\"1\">{number}</text>\
<g transform=\"translate(176 76)\" fill=\"none\" stroke=\"{silver}\" stroke-width=\".9\"><rect x=\"0\" y=\"0\" width=\"22\" height=\"8\" rx=\"2\"/><rect x=\"2.5\" y=\"1.8\" width=\"7\" height=\"4.4\" rx=\"1\" fill=\"{silver}\" stroke=\"none\"/><path d=\"M26 2 h4 M26 6 h4\"/></g>\
<line x1=\"36\" y1=\"102\" x2=\"204\" y2=\"102\" stroke=\"{silver}\" stroke-opacity=\".7\" stroke-width=\".9\"/>\
<g font-size=\"9.5\" font-weight=\"700\" letter-spacing=\".6\">\
<rect x=\"40\" y=\"110\" width=\"11\" height=\"11\" rx=\"1.2\" fill=\"none\" stroke=\"{silver}\" stroke-width=\".9\"/>\
<text x=\"45.5\" y=\"118.6\" text-anchor=\"middle\" font-size=\"8\">A</text>\
<text x=\"58\" y=\"118.8\">{row_a}</text>\
<g opacity=\"{b_opacity}\">\
<rect x=\"40\" y=\"130\" width=\"11\" height=\"11\" rx=\"1.2\" fill=\"none\" stroke=\"{silver}\" stroke-width=\".9\"/>\
<text x=\"45.5\" y=\"138.6\" text-anchor=\"middle\" font-size=\"8\">B</text>\
<text x=\"58\" y=\"138.8\">{row_b}</text>\
</g>\
</g>\
<text x=\"40\" y=\"178\" font-size=\"6.6\" font-weight=\"700\" letter-spacing=\"1.6\" fill=\"#a8a49a\">RF-7 VOICE CARTRIDGE</text>\
<text x=\"204\" y=\"178\" text-anchor=\"end\" font-size=\"6.6\" letter-spacing=\".4\" fill=\"#8f8b82\">{serial}</text>\
<text transform=\"rotate(-90 31 150)\" x=\"31\" y=\"150\" text-anchor=\"middle\" font-size=\"4.6\" letter-spacing=\"1\" fill=\"#6f6c65\">{} VOICES · {banks}</text>\
</g>\
<rect x=\"8\" y=\"8\" width=\"224\" height=\"192\" rx=\"10\" fill=\"url(#cg-{k})\" pointer-events=\"none\"/>\
</svg>",
        face.voices
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> Face {
        Face {
            number: "2".into(),
            kind: Kind::Rom,
            rows: ["BRASS 1 — SYN-LEAD 4".into(), String::new()],
            serial: "ROM1A.syx".into(),
            voices: 32,
            banks: 1,
            dim: false,
        }
    }

    #[test]
    fn a_rom_wears_the_turquoise_band_and_a_ram_the_silver_one() {
        let rom = svg(&face(), "g1");
        assert!(rom.contains(">VOICE</text>") && rom.contains(">ROM</text>"));
        assert!(rom.contains("#33cfac"));
        let ram = svg(
            &Face {
                kind: Kind::Ram,
                ..face()
            },
            "saved",
        );
        assert!(ram.contains(">DATA</text>") && ram.contains(">RAM</text>"));
        assert!(ram.contains("#d9362b"), "the RAM's red");
    }

    #[test]
    fn the_label_is_escaped_cut_and_keyed() {
        let mut long = face();
        long.rows[0] = "A <very> long & \"row\" that runs past the label's edge".into();
        long.serial = "cartridge/with/slashes & more.syx".into();
        let html = svg(&long, "g 1/x");
        assert!(html.contains("A &lt;very&gt; long &amp; &quot;row&quot; that…"));
        assert!(!html.contains("<very>"));
        assert!(html.contains("cartridge/with/slashe…"));
        assert!(html.contains("id=\"cb-g-1-x\"") && html.contains("url(#cb-g-1-x)"));
        assert!(html.contains("32 VOICES · 1 BANK"));
        // An empty row B is printed faintly; a second bank prints it full.
        assert!(html.contains("<g opacity=\".35\">"));
        let mut two = face();
        two.rows[1] = "MORE".into();
        two.banks = 2;
        let html = svg(&two, "g2");
        assert!(html.contains("<g opacity=\"1\">") && html.contains("2 BANKS"));
    }
}
