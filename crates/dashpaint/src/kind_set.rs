//! The document's paint kind set — the census a painter specialises its
//! shading on.
//!
//! # What is in it, and what is deliberately not
//!
//! Two bits: whether the document clips anything, and whether it strokes
//! anything. They are here because they are the two arms of the shading that
//! **exist to be compiled out** — the clip loop, and the stroke band — and a
//! bit that removes no code only multiplies the variant set a painter has to
//! build and keep. A gradient bit was weighed and left out for exactly that
//! reason: since the ramp became a texture sample there is no gradient loop to
//! remove, only a branch that a document with no gradient never takes.
//!
//! # It is a census of the committed tables, not of the file
//!
//! [`KindSet::of`] reads the tables a commit produced, so it answers for the
//! frame about to be drawn rather than for the document that was loaded. That
//! distinction is load-bearing on the painter side: the tables **grow** when a
//! paint or a stroke is interned mid-run, so a painter that chose its pipeline
//! at load and kept it would draw a stroke through shading that has no stroke
//! arm in it. The atlas precedent does not transfer — an atlas set changes only
//! with a load.
//!
//! `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
//! is where per-document specialisation is the one thing that survived the
//! "bake more at import" alternatives.

use crate::{ClipTable, PaintTable};

/// Which of the two compiled-out arms a document reaches.
///
/// `Hash` and `Eq` because a painter caches one pipeline per set and this is
/// the key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct KindSet {
    /// The document clips: some region in the clip table names a box.
    pub clips: bool,
    /// The document strokes: some entry in the paint table interned a stroke.
    pub strokes: bool,
}

impl KindSet {
    /// The census of one commit's tables.
    ///
    /// **The clip bit is over the boxes, not over the regions.** A clip table
    /// always holds the reserved unclipped region at index 0, so its region
    /// count is never zero and a set derived from that would be `true` for
    /// every document ever committed.
    pub fn of(paints: &PaintTable, clips: &ClipTable) -> Self {
        Self {
            clips: !clips.all_boxes().is_empty(),
            strokes: !paints.all_strokes().is_empty(),
        }
    }

    /// The set as two bits — clips at bit 0, strokes at bit 1.
    ///
    /// What crosses the C ABI, because a host that toggles shader keywords
    /// needs one integer rather than a struct, and what
    /// [`from_bits`](Self::from_bits) reads back.
    pub fn bits(&self) -> u32 {
        u32::from(self.clips) | u32::from(self.strokes) << 1
    }

    /// The set a `bits()` value names. Bits above the two named are ignored:
    /// this is the reader of a value a host may have widened, and the two it
    /// knows are the two it answers for.
    pub fn from_bits(bits: u32) -> Self {
        Self {
            clips: bits & 1 != 0,
            strokes: bits & 2 != 0,
        }
    }

    /// The set as WGSL pipeline-override constants — the shape
    /// `wgpu::PipelineCompilationOptions::constants` takes.
    ///
    /// The names are the `override` declarations at the top of
    /// `crates/dashscene-gpu/src/shaders/paint.wgsl`. A name that does not
    /// match one there is a pipeline-creation error rather than a silent
    /// default, which is what makes this pair safe to state twice.
    pub fn constants(&self) -> [(&'static str, f64); 2] {
        [
            ("HAS_CLIPS", f64::from(u8::from(self.clips))),
            ("HAS_STROKES", f64::from(u8::from(self.strokes))),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClipBox, CornerRadii, EntryParts, PaintEntry, Stroke, StrokeAlign};

    /// A clip table holding one box, which is what makes the clip bit true.
    fn clips_one_box() -> ClipTable {
        let mut clips = ClipTable::default();
        clips.push(&[ClipBox {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            corners: CornerRadii::default(),
        }]);
        clips
    }

    /// A paint table holding one entry that interned a stroke, which is what
    /// makes the stroke bit true.
    fn paints_one_stroke() -> PaintTable {
        let mut paints = PaintTable::default();
        paints.push_with(
            PaintEntry::default(),
            EntryParts {
                stroke: Some(Stroke {
                    width: 2.0,
                    align: StrokeAlign::Center,
                    color: crate::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    },
                }),
                ..Default::default()
            },
        );
        paints
    }

    #[test]
    fn an_empty_table_has_no_kind() {
        let kinds = KindSet::of(&PaintTable::default(), &ClipTable::default());
        assert_eq!(kinds, KindSet::default());
        assert_eq!(
            kinds.constants(),
            [("HAS_CLIPS", 0.0), ("HAS_STROKES", 0.0)]
        );
        assert_eq!(kinds.bits(), 0);
    }

    #[test]
    fn each_bit_is_its_own_and_round_trips() {
        // Each table alone, so a census that read one table for both bits — or
        // read the two the other way round — fails rather than passing on a
        // document that has both.
        let only_clips = KindSet::of(&PaintTable::default(), &clips_one_box());
        assert!(
            only_clips.clips && !only_clips.strokes,
            "{only_clips:?} from a clip box alone"
        );
        assert_eq!(
            only_clips.constants(),
            [("HAS_CLIPS", 1.0), ("HAS_STROKES", 0.0)]
        );
        assert_eq!(only_clips.bits(), 1);

        let only_strokes = KindSet::of(&paints_one_stroke(), &ClipTable::default());
        assert!(
            !only_strokes.clips && only_strokes.strokes,
            "{only_strokes:?} from a stroke alone"
        );
        assert_eq!(
            only_strokes.constants(),
            [("HAS_CLIPS", 0.0), ("HAS_STROKES", 1.0)]
        );
        assert_eq!(only_strokes.bits(), 2);

        let both = KindSet::of(&paints_one_stroke(), &clips_one_box());
        assert_eq!(
            both,
            KindSet {
                clips: true,
                strokes: true
            }
        );
        assert_eq!(both.bits(), 3);
        assert_eq!(both.constants(), [("HAS_CLIPS", 1.0), ("HAS_STROKES", 1.0)]);
        // Bit 1 alone read back, so the two bits cannot be swapped in
        // `from_bits` without failing here either.
        assert_eq!(KindSet::from_bits(2), only_strokes);
        assert_eq!(KindSet::from_bits(1), only_clips);
        assert_eq!(KindSet::from_bits(both.bits()), both);
        // Bits above the two named are dropped rather than corrupting the two,
        // which is what lets a host widen the value without breaking a reader.
        assert_eq!(KindSet::from_bits(u32::MAX), both);
    }
}
