//! Kitty graphics for the picture viewer, written directly: each frame is
//! sent as a PNG (a diagram's flat colours compress to a few tens of KB,
//! where raw pixels for a retina screen are tens of MB) with the cell area
//! it fills (`c`, `r`: the terminal scales it), under one image id that the
//! next frame replaces, so frames don't pile up in the terminal's memory.
//! It's shown with unicode placeholders, like ratatui-image's pictures.
//!
//! <https://sw.kovidgoyal.net/kitty/graphics-protocol/>

use std::fmt::Write as _;

use base64::Engine as _;
use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;

/// The character kitty reserves for image placeholders.
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// A picture to put on screen: sent once, then placeholders.
pub struct KittyFrame {
    id: u32,
    pub cols: u16,
    pub rows: u16,
    /// The transmission, until it's been drawn once.
    transmit: Option<String>,
}

impl KittyFrame {
    /// `png` filling `cols`×`rows` cells under image `id` (1..2^24: the id
    /// is told by the placeholders' colour, 24 bits).
    pub fn new(png: &[u8], id: u32, cols: u16, rows: u16) -> KittyFrame {
        let (cols, rows) = (cols.min(MAX_CELLS), rows.min(MAX_CELLS));
        KittyFrame {
            id,
            cols,
            rows,
            transmit: Some(transmit(png, id, cols, rows)),
        }
    }

    /// Draws the placeholders at `area`'s top left (the transmission with
    /// the first cell, the first time).
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Placeholders tell the image by their colour: the id as 0xRRGGBB.
        let fg = crate::style::rgb_u32(self.id);
        let (w, h) = (self.cols.min(area.width), self.rows.min(area.height));
        let mut send = self.transmit.take();
        for y in 0..h {
            for x in 0..w {
                let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) else {
                    continue;
                };
                let mut symbol = send.take().unwrap_or_default();
                symbol.push(PLACEHOLDER);
                // The first cell of a row says where it is; the rest follow on.
                if x == 0 {
                    symbol.push(DIACRITICS[usize::from(y)]);
                    symbol.push(DIACRITICS[0]);
                }
                // One column each, whatever the escape sequences in it add up to.
                cell.set_symbol(&symbol)
                    .set_fg(fg)
                    .set_diff_option(CellDiffOption::ForcedWidth(std::num::NonZeroU16::MIN));
            }
        }
    }
}

/// Cells a placement can span: positions past the diacritics table can't be told.
const MAX_CELLS: u16 = DIACRITICS.len() as u16;

/// The escape sequences that send `png` as image `id` with a virtual
/// placement (`U=1`) `cols`×`rows` cells big, in chunks of 4096 base64
/// characters (the protocol's limit).
pub fn transmit(png: &[u8], id: u32, cols: u16, rows: u16) -> String {
    let data = base64::engine::general_purpose::STANDARD.encode(png);
    let chunks: Vec<&str> = data
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect();
    let mut out = String::with_capacity(data.len() + chunks.len() * 16 + 64);
    for (i, chunk) in chunks.iter().enumerate() {
        out.push_str("\x1b_Gq=2,");
        if i == 0 {
            let _ = write!(out, "i={id},a=T,U=1,f=100,c={cols},r={rows},");
        }
        let _ = write!(out, "m={};{chunk}\x1b\\", u8::from(i + 1 < chunks.len()));
    }
    out
}

/// Row and column numbers for placeholders, from kitty's
/// rowcolumn-diacritics.txt (as in ratatui-image, MIT).
static DIACRITICS: [char; 297] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_a_png_scaled_to_cells_in_chunks() {
        let png = vec![7u8; 10_000];
        let t = transmit(&png, 42, 30, 10);
        assert!(
            t.starts_with("\x1b_Gq=2,i=42,a=T,U=1,f=100,c=30,r=10,m=1;"),
            "{}",
            &t[..60]
        );
        let chunks: Vec<&str> = t.split("\x1b_G").filter(|s| !s.is_empty()).collect();
        assert_eq!(chunks.len(), 4, "13336 base64 characters: 4 chunks");
        assert!(
            chunks.last().unwrap().starts_with("q=2,m=0;"),
            "the last says it's over"
        );
        let data: String = chunks
            .iter()
            .map(|c| c.split_once(';').unwrap().1.trim_end_matches("\x1b\\"))
            .collect();
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(data).unwrap(), png);
    }

    #[test]
    fn placeholders_fill_the_area_and_the_first_draw_sends() {
        let mut f = KittyFrame::new(b"png", 0x010203, 3, 2);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        f.render(Rect::new(1, 1, 10, 5), &mut buf);
        let first = buf[(1, 1)].symbol();
        assert!(first.starts_with("\x1b_G") && first.contains("i=66051"));
        assert!(first.ends_with(&format!("{PLACEHOLDER}{}{}", DIACRITICS[0], DIACRITICS[0])));
        assert_eq!(
            buf[(1, 2)].symbol(),
            format!("{PLACEHOLDER}{}{}", DIACRITICS[1], DIACRITICS[0])
        );
        assert_eq!(buf[(3, 2)].symbol(), PLACEHOLDER.to_string());
        assert_eq!(buf[(4, 2)].symbol(), " ", "3 columns only");
        assert_eq!(buf[(1, 1)].fg, crate::style::rgb_u32(0x010203));
        // Drawn again: placeholders only, nothing sent twice.
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        f.render(Rect::new(1, 1, 10, 5), &mut buf);
        assert!(!buf[(1, 1)].symbol().contains("\x1b"));
    }
}
