//! The `dashpack` binary — the standalone-tool half of the crate.
//!
//! The standalone-tool requirement is met by this artifact
//! (`cargo build -p dashpack`), not by the packer living in its own repo.

fn main() -> std::process::ExitCode {
    eprintln!(
        "dashpack {}: no packing operation is implemented yet.",
        dashpack::version()
    );
    eprintln!(
        "Story #429 registers the crate and its name; the encoder, the container writer, the band oracle and cold-bank assembly land across the rest of epic #345."
    );
    // Non-zero: a packer that packed nothing has not succeeded, and a caller
    // that starts scripting against it should find that out now rather than
    // from an empty bank.
    std::process::ExitCode::FAILURE
}
