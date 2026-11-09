//! OpenType GPOS pair-kerning extraction.
//!
//! Fontdue only reads the legacy `kern` table, but many modern fonts (including
//! Torn's bundled Ubuntu Light) store kerning exclusively in GPOS. This module
//! finds the lookups referenced by `kern` features and resolves individual
//! glyph pairs on demand, so no large pair table has to be materialized.

use ttf_parser::{
    Face, GlyphId, Tag,
    gpos::{PairAdjustment, PositioningSubtable, ValueRecord},
    opentype_layout::LayoutTable,
};

/// Collects the GPOS lookup indices referenced by `kern` features.
///
/// The lookups are gathered across every script and language in the font, then
/// sorted and deduplicated. Returns an empty vector when the font has no GPOS
/// table, no `kern` feature, or cannot be parsed by `ttf-parser`.
pub(crate) fn collect_kern_lookups(data: &[u8]) -> Vec<u16> {
    let Ok(face) = Face::parse(data, 0) else {
        return Vec::new();
    };
    let Some(gpos) = face.tables().gpos else {
        return Vec::new();
    };

    let kern = Tag::from_bytes(b"kern");
    let mut lookups = Vec::new();
    for script in gpos.scripts {
        if let Some(language) = script.default_language {
            collect_language(&gpos, &language, kern, &mut lookups);
        }
        for language in script.languages {
            collect_language(&gpos, &language, kern, &mut lookups);
        }
    }
    lookups.sort_unstable();
    lookups.dedup();
    lookups
}

fn collect_language(
    gpos: &LayoutTable<'_>,
    language: &ttf_parser::opentype_layout::LanguageSystem<'_>,
    kern: Tag,
    lookups: &mut Vec<u16>,
) {
    if let Some(index) = language.required_feature {
        collect_feature(gpos, index, kern, lookups);
    }
    for index in 0..language.feature_indices.len() {
        if let Some(index) = language.feature_indices.get(index) {
            collect_feature(gpos, index, kern, lookups);
        }
    }
}

fn collect_feature(gpos: &LayoutTable<'_>, index: u16, kern: Tag, lookups: &mut Vec<u16>) {
    let Some(feature) = gpos.features.get(index) else {
        return;
    };
    if feature.tag != kern {
        return;
    }
    for index in 0..feature.lookup_indices.len() {
        if let Some(lookup) = feature.lookup_indices.get(index) {
            lookups.push(lookup);
        }
    }
}

/// A horizontal pair adjustment split by how it affects the glyph stream.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KerningAdjustment {
    /// Added to the first glyph's advance, moving the second glyph and all
    /// glyphs after it.
    pub advance: f32,
    /// Moves the second glyph without changing the following advance.
    pub placement: f32,
}

impl KerningAdjustment {
    /// Converts design units to logical pixels.
    pub(crate) fn scaled(self, factor: f32) -> Self {
        Self {
            advance: self.advance * factor,
            placement: self.placement * factor,
        }
    }
}

/// Returns the horizontal pair adjustment, in design units, for two glyphs.
///
/// Every `kern` lookup that applies to the pair contributes its first-glyph
/// horizontal advance and second-glyph horizontal placement. Within one lookup
/// only the first matching subtable is used, matching OpenType application
/// order.
pub(crate) fn gpos_kerning(
    face: &Face<'_>,
    lookup_indices: &[u16],
    left: u16,
    right: u16,
) -> Option<KerningAdjustment> {
    let gpos = face.tables().gpos?;
    let left = GlyphId(left);
    let right = GlyphId(right);
    let mut adjustment = KerningAdjustment::default();
    let mut matched = false;
    for &lookup_index in lookup_indices {
        let Some(lookup) = gpos.lookups.get(lookup_index) else {
            continue;
        };
        for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
            let PositioningSubtable::Pair(pair) = subtable else {
                continue;
            };
            if let Some((first, second)) = pair_adjustment(&pair, left, right) {
                adjustment.advance += f32::from(first.x_advance);
                adjustment.placement += f32::from(second.x_placement);
                matched = true;
                break;
            }
        }
    }
    matched.then_some(adjustment)
}

fn pair_adjustment<'a>(
    pair: &PairAdjustment<'a>,
    left: GlyphId,
    right: GlyphId,
) -> Option<(ValueRecord<'a>, ValueRecord<'a>)> {
    match pair {
        PairAdjustment::Format1 { coverage, sets } => {
            let index = coverage.get(left)?;
            let set = sets.get(index)?;
            set.get(right)
        }
        PairAdjustment::Format2 {
            coverage,
            classes,
            matrix,
        } => {
            coverage.get(left)?;
            let first = classes.0.get(left);
            let second = classes.1.get(right);
            matrix.get((first, second))
        }
    }
}
