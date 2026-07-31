//! Which scene the host draws (story #574).
//!
//! The scenes themselves live in `corpus/showcase/`, not here: they exercise
//! the full v0 paint vocabulary, which is what the stress corpus is for, and
//! keeping a second set beside the host would be two sets that drift
//! (epic #568). This module is only the selection.
//!
//! Selection is a command-line argument rather than a key, because story #573
//! owns input and a scene picker that fought it would be two mechanisms for
//! one job:
//!
//! ```text
//! cargo run -p demo                # the default scene
//! cargo run -p demo -- typography  # a named one
//! cargo run -p demo -- --all       # every scene in turn, cycling
//! cargo run -p demo -- --list      # what there is
//! ```

use showcase::Showcase;

/// What the caller asked for.
pub enum Selection {
    /// Draw this scene.
    Scene(&'static Showcase),
    /// Draw every scene in turn, cycling (issue #628). The host advances on
    /// its own, so the vocabulary checklist can be walked in one run instead
    /// of one run per scene.
    All,
    /// Print the scene list and exit successfully — the caller asked for it.
    Listed,
    /// The named scene does not exist. The list has been printed; exit with a
    /// failure, because the run did not do what was asked.
    Unknown,
}

/// Resolves the process arguments to a scene.
pub fn select(arguments: impl IntoIterator<Item = String>) -> Selection {
    let requested = arguments.into_iter().next();
    match requested.as_deref() {
        Some("--list" | "-l") => {
            list();
            Selection::Listed
        }
        Some("--all" | "-a") => Selection::All,
        Some(name) => match showcase::by_name(name) {
            Some(scene) => Selection::Scene(scene),
            None => {
                eprintln!("demo: no scene named {name:?}");
                list();
                Selection::Unknown
            }
        },
        None => Selection::Scene(
            showcase::by_name(showcase::DEFAULT).expect("the default scene is one of the scenes"),
        ),
    }
}

fn list() {
    eprintln!("demo: scenes are");
    for scene in showcase::SCENES {
        eprintln!("demo:   {:<12} {}", scene.name, scene.summary);
    }
}

#[cfg(test)]
mod tests {
    use super::{Selection, select};

    /// The default has to resolve, or `cargo run -p demo` with no arguments
    /// opens a window with nothing in it.
    #[test]
    fn no_argument_selects_the_default_scene() {
        let Selection::Scene(scene) = select([]) else {
            panic!("an empty argument list selects a scene");
        };
        assert_eq!(scene.name, showcase::DEFAULT);
    }

    #[test]
    fn a_name_selects_the_scene_that_carries_it() {
        let Selection::Scene(scene) = select(["layout".to_owned()]) else {
            panic!("a known name selects a scene");
        };
        assert_eq!(scene.name, "layout");
    }

    #[test]
    fn an_unknown_name_is_refused_rather_than_falling_back() {
        assert!(matches!(
            select(["nonesuch".to_owned()]),
            Selection::Unknown
        ));
    }

    #[test]
    fn the_all_flag_asks_for_every_scene() {
        assert!(matches!(select(["--all".to_owned()]), Selection::All));
        assert!(matches!(select(["-a".to_owned()]), Selection::All));
    }

    /// The host advances only when it holds more than one scene, so a reel of
    /// one would silently be an ordinary single-scene run. Nothing in the code
    /// stops the scene list shrinking to one, so this is asserted rather than
    /// assumed (issue #628).
    #[test]
    fn every_scene_is_more_than_one_scene() {
        assert!(
            showcase::SCENES.len() > 1,
            "`--all` advances only with more than one scene; with one it would \
             be an ordinary run under a different name"
        );
    }
}
