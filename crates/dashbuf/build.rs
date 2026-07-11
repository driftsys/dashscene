use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let schema = "schema/dashbuf.fbs";
    println!("cargo:rerun-if-changed={schema}");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    let status = Command::new("flatc")
        .args(["--rust", "-o"])
        .arg(&out_dir)
        .arg(schema)
        .status()
        .expect(
            "failed to run flatc — install it (e.g. `brew install flatbuffers` \
             or apt-get install flatbuffers-compiler) and ensure it's on PATH",
        );

    if !status.success() {
        panic!("flatc exited with {status}");
    }
}
