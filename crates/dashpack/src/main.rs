//! The `dashpack` binary — the standalone-tool half of the crate.
//!
//! The standalone-tool requirement is met by this artifact
//! (`cargo build -p dashpack`), not by the packer living in its own repo.

fn main() -> std::process::ExitCode {
    eprintln!(
        "dashpack {}: no packing operation is implemented yet.",
        dashpack::version()
    );
    // The encoder pin, read off the artifact rather than off the source tree.
    // Two banks are only comparable if the same encoder produced them.
    let (astcenc_version, astcenc_commit) = dashpack::astc::vendored_astcenc();
    eprintln!("Links astcenc {astcenc_version} (commit {astcenc_commit}), vendored in-tree.");
    // The container pin, for the same reason. This one is also written into
    // every emitted file's KTXwriter key, so reporting it here lets the two be
    // compared without opening a file.
    eprintln!(
        "Writes KTX2 as \"{}\", Zstd level {}.",
        dashpack::ktx2::WRITER,
        dashpack::ktx2::ZSTD_LEVEL
    );
    eprintln!(
        "Story #429 registers the crate and its name, story #430 adds ASTC encode and the matching reference decode, story #431 adds the KTX2 container writer; the band oracle and cold-bank assembly land across the rest of epic #345."
    );
    // Non-zero: a packer that packed nothing has not succeeded, and a caller
    // that starts scripting against it should find that out now rather than
    // from an empty bank.
    std::process::ExitCode::FAILURE
}
