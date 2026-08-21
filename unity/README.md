# unity/

The Unity C# package, and the check that holds it to the Rust build.

    com.driftsys.dashscene/   the UPM package — declarations, no painter
    abi-check/                a plain .NET check, no Unity editor needed
    package-compat/           a netstandard2.1 compile of the package's
                              Runtime/, which abi-check cannot do — it targets
                              net10.0, a superset of what Unity accepts

Sited in this repository rather than in a separate one by the owner's ruling of
2026-08-17, recorded in
[`../docs/decisions/unity-package-sited-in-this-repository.md`](../docs/decisions/unity-package-sited-in-this-repository.md).
UPM installs from a Git URL with `?path=`, so a subfolder is directly
consumable.

**Sharing a repository gains nothing on its own** — that record says so, and the
two checks here are what give it value. `just unity-abi` runs both.

This directory is outside the Cargo workspace, as `importers/` is, and carries
its own `unity` commit scope in `.git-std.toml`.
