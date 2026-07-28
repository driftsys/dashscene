//! The shared design-source oracle harness (debt #338).
//!
//! Two oracles measure a render against Figma's own `GET /images` export: the
//! E7 exit gate (`render_oracle.rs`, `goldens/oracle/manifest.json`) and the
//! import-fidelity oracle (`import_oracle.rs`,
//! `goldens/oracle/import-manifest.json`). They walk their manifests the same
//! way, and until now each carried its own copy of that walk.
//!
//! The copies were deliberate. The E7 surface was frozen until the v0.9 exit
//! gate closed, so the import oracle mirrored it rather than reaching into it
//! (#332, PR #337). Issue #49 closed on 2026-07-25, so the freeze is gone and
//! the two walks collapse into this one.
//!
//! What stays per-oracle is what genuinely differs: how a frame renders (the
//! E7 gate compiles a fixture with an empty images map and default text axes;
//! the import oracle scopes to the exported node and renders embedded image
//! bytes through the production path), and the manifest's own gate field, which
//! the two spell differently — `gate.issue` against a top-level `issue`.
//!
//! This lives in `tests/common/` rather than the `goldens` library because
//! `serde_json` is a dev-dependency: a manifest walk is a test concern, and
//! promoting the dependency to build it into the shipped library would be the
//! wrong trade (debt #120 names this directory as the shared home).

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use goldens::oracle::{self, ExcludeRegion};
use serde_json::Value;

/// The `goldens/` root — one level up from this crate (`goldens/tooling`).
pub fn goldens_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The repository root — two levels up from this crate. A frame's `fixture`
/// path is repo-relative (`corpus/figma-fixtures/<name>.json`), unlike the
/// goldens-relative `designSource`.
pub fn repo_root() -> PathBuf {
    goldens_root().join("..")
}

/// A frame's optional `excludeRegions`: rectangles whose pixels the diff drops
/// from both the differing count and the total. Absent or empty means no
/// exclusion. Each region is `{x, y, w, h}` in the render's pixel coordinates;
/// a missing or non-integer component is a manifest error, not a silent skip.
pub fn exclude_regions(frame: &Value) -> Vec<ExcludeRegion> {
    let name = frame["frame"].as_str().unwrap_or("<unnamed>");
    let Some(regions) = frame.get("excludeRegions") else {
        return Vec::new();
    };
    let regions = regions
        .as_array()
        .unwrap_or_else(|| panic!("frame {name}'s excludeRegions must be an array"));
    regions
        .iter()
        .map(|region| {
            let component = |key: &str| {
                region[key]
                    .as_i64()
                    .unwrap_or_else(|| panic!("frame {name}'s excludeRegion needs integer {key}"))
                    as i32
            };
            ExcludeRegion {
                x: component("x"),
                y: component("y"),
                w: component("w"),
                h: component("h"),
            }
        })
        .collect()
}

/// One oracle's manifest, plus the two strings that distinguish its report
/// from the other's: the `status` a frame with no design source must carry,
/// and the label its measured lines are printed under.
pub struct OracleManifest {
    value: Value,
    pending_status: &'static str,
    label: &'static str,
}

impl OracleManifest {
    /// Loads a manifest by its `goldens/`-relative path.
    pub fn load(
        goldens_relative: &str,
        pending_status: &'static str,
        label: &'static str,
    ) -> OracleManifest {
        let path = goldens_root().join(goldens_relative);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("oracle manifest {} present: {e}", path.display()));
        let value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("oracle manifest {} parses: {e}", path.display()));
        OracleManifest {
            value,
            pending_status,
            label,
        }
    }

    pub fn frames(&self) -> &Vec<Value> {
        self.value["frames"]
            .as_array()
            .expect("the manifest has a frames array")
    }

    /// The manifest's own JSON, for the gate assertions the two oracles spell
    /// differently.
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Every frame names one of the three pinned bands, and a frame that
    /// declares a fixture ships it.
    ///
    /// The `band_for` round-trip is asserted here rather than in each oracle:
    /// a band that resolved to a rule other than its own name would grade every
    /// frame against the wrong budget, and one copy of that check is enough.
    pub fn assert_bands_and_fixtures(&self) {
        let repo = repo_root();
        assert!(!self.frames().is_empty(), "the manifest lists frames");

        for frame in self.frames() {
            let name = frame["frame"].as_str().expect("frame name");
            let band = frame["band"].as_str().expect("band name");
            let resolved = oracle::band_for(band).unwrap_or_else(|| {
                panic!("frame {name} names band {band}, which is not one of the pinned rules")
            });
            assert_eq!(
                resolved.rule, band,
                "band_for({band}) must return the band whose rule matches the name, \
                 not a mis-mapped band"
            );
            // A frame's `fixture` is the committed Figma fixture the oracle
            // imports and renders. A pending frame with no renderable fixture
            // carries null; a frame that names one must ship it.
            if let Some(fixture) = frame["fixture"].as_str() {
                let path = repo.join(fixture);
                assert!(
                    path.exists(),
                    "frame {name}'s fixture {} is not committed",
                    path.display()
                );
            }
        }
    }

    /// A frame with no committed design source says so through its `status`,
    /// and a frame that has one ships the file.
    ///
    /// This checks each frame's own state, never "all frames are pending", so
    /// it stays valid as frames are captured.
    pub fn assert_captured_or_pending(&self) {
        let root = goldens_root();
        for frame in self.frames() {
            let name = frame["frame"].as_str().expect("frame name");
            match frame["designSource"].as_str() {
                None => assert_eq!(
                    frame["status"].as_str(),
                    Some(self.pending_status),
                    "frame {name} has no design source, so it must be marked {}",
                    self.pending_status
                ),
                Some(source) => {
                    let path = root.join(source);
                    assert!(
                        path.exists(),
                        "frame {name} declares design source {} but the file is not committed",
                        path.display()
                    );
                    assert_eq!(
                        frame["status"].as_str(),
                        Some("captured"),
                        "frame {name} has a design source, so its status must be captured, \
                         not a stale {}",
                        self.pending_status
                    );
                }
            }
        }
    }

    /// The fidelity assertion itself: every frame with a committed design
    /// source is rendered by `render` and diffed against that source within the
    /// frame's band. Frames with no design source are counted as pending.
    ///
    /// `render` takes the frame and returns the reference PNG, which is the
    /// only part the two oracles do differently.
    ///
    /// Panics listing every frame outside its band. The measured lines are
    /// printed either way, so the `render-oracle` CI job's `--nocapture` run
    /// still shows the per-frame numbers.
    pub fn measure(&self, mut render: impl FnMut(&Value) -> Vec<u8>) {
        let root = goldens_root();
        let mut measured: Vec<String> = Vec::new();
        let mut pending: Vec<String> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        for frame in self.frames() {
            let name = frame["frame"].as_str().expect("frame name").to_string();
            let band_name = frame["band"].as_str().expect("band name");
            let band = oracle::band_for(band_name)
                .unwrap_or_else(|| panic!("frame {name} names unknown band {band_name}"));

            let Some(source) = frame["designSource"].as_str() else {
                pending.push(name);
                continue;
            };
            let source_bytes = std::fs::read(root.join(source))
                .unwrap_or_else(|e| panic!("frame {name} design source {source}: {e}"));
            // The reference is our own render of the committed fixture, not a
            // pre-committed golden — the correct oracle (G-11, G-23).
            let reference_bytes = render(frame);

            // A frame may declare `excludeRegions` — rectangles carrying a
            // genuine, disclosed structural divergence the area budget must not
            // silently absorb. Excluded pixels count toward neither the
            // differing count nor the total, so the frame measures the rest.
            let exclude = exclude_regions(frame);
            let d = oracle::diff_excluding(&reference_bytes, &source_bytes, band, &exclude)
                .unwrap_or_else(|e| panic!("frame {name}: {e}"));
            let excluded_note = if exclude.is_empty() {
                String::new()
            } else {
                format!(" [{} region(s) excluded]", exclude.len())
            };
            // A gated band reports both terms, always — not only the one that
            // failed. A report that showed the gate only when it fired would
            // leave a reader unable to tell a frame with no gate from one
            // comfortably inside it, and the gate's headroom is the number that
            // says whether it still binds (issue #422).
            let gate_note = match &band.gate {
                Some(gate) => format!(
                    ", gate {}/{} px ({:.3}%) vs {:.1}%",
                    d.gate_differing,
                    d.total,
                    d.gate_fraction() * 100.0,
                    gate.differing_fraction * 100.0,
                ),
                None => String::new(),
            };
            let line = format!(
                "{name}: {}/{} px differ ({:.3}%, max Δ {}) vs the {} band's {:.1}% budget{gate_note}{excluded_note}",
                d.differing,
                d.total,
                d.fraction() * 100.0,
                d.max_channel_delta,
                band.rule,
                band.differing_fraction * 100.0,
            );
            if !d.passes() {
                failures.push(line.clone());
            }
            measured.push(line);
        }

        eprintln!(
            "{}: {} frame(s) measured against a Figma design source, {} pending{}",
            self.label,
            measured.len(),
            pending.len(),
            if pending.is_empty() {
                String::new()
            } else {
                format!(" ({})", pending.join(", "))
            }
        );
        for line in &measured {
            eprintln!("  {line}");
        }

        // Test-lock the report's honesty: `assert!(failures.is_empty())` alone
        // passes even when nothing was measured, so the accounting is asserted
        // too — every frame is either measured against a real design source or
        // pending, and none is silently dropped. This is NOT
        // `assert!(pending.is_empty())`: whether a pending frame is allowed is
        // the owning gate's question, not this harness's.
        assert_eq!(
            measured.len() + pending.len(),
            self.frames().len(),
            "every manifest frame must be measured or pending — none silently dropped"
        );
        assert!(
            failures.is_empty(),
            "design-source fidelity failures:\n{}",
            failures.join("\n")
        );
    }
}
