//! Glyph advances of a font, read to place extracted text on lines.
//!
//! Only tables whose codes are known without a CMap are used: the `/Widths`
//! of a simple font (one byte per code) and the `/W` and `/DW` of a Type0
//! font with the `Identity-H` encoding (two bytes per code). Any other or
//! malformed font has unknown advances and the caller falls back.

use anyhow::Result;
use lopdf::{Dictionary, Document, Object};

use super::super::resource::Budget;

/// Most `/W` elements read for one font; a longer table is not used.
const MAX_CID_ELEMENTS: usize = 262_144;

pub(super) enum Widths {
    Unknown,
    Simple {
        first: usize,
        widths: Vec<f32>,
        missing: f32,
    },
    Cid {
        default: f32,
        /// Sorted by first CID.
        ranges: Vec<Range>,
    },
}

/// CIDs `first..=last`. One width applies to all of them; more are one each.
pub(super) struct Range {
    first: u32,
    last: u32,
    widths: Vec<f32>,
}

/// The glyphs of one string: summed widths in thousandths of the font size.
#[derive(Default, Clone, Copy)]
pub(super) struct Measure {
    pub(super) thousandths: f64,
    pub(super) glyphs: u64,
    /// Single-byte codes 32, to which word spacing applies.
    pub(super) spaces: u64,
}

impl Widths {
    pub(super) fn read(doc: &Document, font: &Dictionary, budget: &mut Budget<'_>) -> Result<Self> {
        let subtype = font
            .get(b"Subtype")
            .and_then(Object::as_name)
            .unwrap_or(b"");
        Ok(match subtype {
            b"Type0" => cid(doc, font, budget)?.unwrap_or(Self::Unknown),
            // Type3 widths are in glyph space, scaled by the font matrix.
            b"Type3" => Self::Unknown,
            _ => simple(doc, font, budget)?.unwrap_or(Self::Unknown),
        })
    }

    pub(super) fn measure(&self, bytes: &[u8]) -> Option<Measure> {
        let mut measure = Measure::default();
        match self {
            Self::Unknown => return None,
            Self::Simple {
                first,
                widths,
                missing,
            } => {
                for byte in bytes {
                    let width = usize::from(*byte)
                        .checked_sub(*first)
                        .and_then(|index| widths.get(index))
                        .unwrap_or(missing);
                    measure.thousandths += f64::from(*width);
                    measure.spaces += u64::from(*byte == b' ');
                }
                measure.glyphs = bytes.len() as u64;
            }
            Self::Cid { default, ranges } => {
                let (codes, rest) = bytes.as_chunks::<2>();
                if !rest.is_empty() {
                    return None;
                }
                for code in codes {
                    let cid = u32::from(u16::from_be_bytes(*code));
                    let index = ranges.partition_point(|range| range.first <= cid);
                    let width = index
                        .checked_sub(1)
                        .and_then(|index| ranges.get(index))
                        .filter(|range| cid <= range.last)
                        .and_then(|range| match range.widths.as_slice() {
                            [width] => Some(width),
                            widths => widths.get((cid - range.first) as usize),
                        })
                        .unwrap_or(default);
                    measure.thousandths += f64::from(*width);
                }
                measure.glyphs = codes.len() as u64;
            }
        }
        Some(measure)
    }
}

fn number(object: &Object) -> Option<f32> {
    let value = match object {
        Object::Integer(value) => *value as f32,
        Object::Real(value) => *value,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

fn simple(doc: &Document, font: &Dictionary, budget: &mut Budget<'_>) -> Result<Option<Widths>> {
    let Ok(array) = font.get_deref(b"Widths", doc).and_then(Object::as_array) else {
        return Ok(None);
    };
    let Some(first) = font
        .get_deref(b"FirstChar", doc)
        .and_then(Object::as_i64)
        .ok()
        .and_then(|first| usize::try_from(first).ok())
    else {
        return Ok(None);
    };
    // A simple font has 256 codes.
    let array = &array[..array.len().min(256)];
    budget.charge_items(array.len() as u64, "PDF font widths")?;
    let Some(widths) = array.iter().map(number).collect::<Option<Vec<_>>>() else {
        return Ok(None);
    };
    let missing = font
        .get_deref(b"FontDescriptor", doc)
        .and_then(Object::as_dict)
        .and_then(|descriptor| descriptor.get_deref(b"MissingWidth", doc))
        .ok()
        .and_then(number)
        .unwrap_or(0.0);
    Ok(Some(Widths::Simple {
        first,
        widths,
        missing,
    }))
}

fn cid(doc: &Document, font: &Dictionary, budget: &mut Budget<'_>) -> Result<Option<Widths>> {
    if !matches!(
        font.get_deref(b"Encoding", doc).and_then(Object::as_name),
        Ok(b"Identity-H")
    ) {
        return Ok(None);
    }
    let Some(descendant) = font
        .get_deref(b"DescendantFonts", doc)
        .and_then(Object::as_array)
        .ok()
        .and_then(|fonts| fonts.first())
        .and_then(|font| match font {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            font => font.as_dict().ok(),
        })
    else {
        return Ok(None);
    };
    let default = descendant
        .get_deref(b"DW", doc)
        .ok()
        .and_then(number)
        .unwrap_or(1000.0);
    let mut ranges = Vec::new();
    if let Ok(table) = descendant.get_deref(b"W", doc).and_then(Object::as_array) {
        let Some(parsed) = cid_ranges(table, budget)? else {
            return Ok(None);
        };
        ranges = parsed;
    }
    ranges.sort_by_key(|range| range.first);
    Ok(Some(Widths::Cid { default, ranges }))
}

/// `/W` holds `first [w …]` and `first last w` records.
fn cid_ranges(table: &[Object], budget: &mut Budget<'_>) -> Result<Option<Vec<Range>>> {
    let cid = |object: &Object| object.as_i64().ok().and_then(|cid| u32::try_from(cid).ok());
    let mut ranges = Vec::new();
    let mut elements = 0_usize;
    let mut rest = table;
    while let [first, second, tail @ ..] = rest {
        budget.charge_item("PDF font width records")?;
        let Some(first) = cid(first) else {
            return Ok(None);
        };
        let (last, widths, tail) = match (second, tail) {
            (Object::Array(list), tail) if !list.is_empty() => {
                elements = elements.saturating_add(list.len());
                if elements > MAX_CID_ELEMENTS {
                    return Ok(None);
                }
                budget.charge_items(list.len() as u64, "PDF font widths")?;
                let Some(widths) = list.iter().map(number).collect::<Option<Vec<_>>>() else {
                    return Ok(None);
                };
                let last = first.saturating_add(widths.len() as u32 - 1);
                (last, widths, tail)
            }
            (last, [width, tail @ ..]) => {
                elements = elements.saturating_add(1);
                let (Some(last), Some(width)) = (cid(last), number(width)) else {
                    return Ok(None);
                };
                if last < first || elements > MAX_CID_ELEMENTS {
                    return Ok(None);
                }
                (last, vec![width], tail)
            }
            _ => return Ok(None),
        };
        ranges.push(Range {
            first,
            last,
            widths,
        });
        rest = tail;
    }
    Ok(rest.is_empty().then_some(ranges))
}
