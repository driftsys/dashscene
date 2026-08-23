//! Writes the generated HLSL. `just sdf-hlsl` runs this.
//!
//! The test beside it checks; this writes. They call one function, so the file
//! a developer commits is byte-identical to the one the test re-derives.

fn main() {
    let root = package_gate::root();
    let wgsl_path = root.join(package_gate::WGSL_PATH);
    let hlsl_path = root.join(package_gate::HLSL_PATH);

    let wgsl = match std::fs::read_to_string(&wgsl_path) {
        Ok(source) => source,
        Err(e) => {
            eprintln!("sdf-hlsl: cannot read {}: {e}", wgsl_path.display());
            std::process::exit(1);
        }
    };

    let hlsl = match package_gate::generate_hlsl(&wgsl) {
        Ok(hlsl) => hlsl,
        Err(e) => {
            eprintln!("sdf-hlsl: {e}");
            std::process::exit(1);
        }
    };

    if let Some(parent) = hlsl_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("sdf-hlsl: cannot create {}: {e}", parent.display());
        std::process::exit(1);
    }

    // Unchanged output is not rewritten. Unity's importer keys on the file's
    // modification time, so rewriting identical bytes costs a reimport of the
    // shader in every project that has the package open.
    if std::fs::read_to_string(&hlsl_path).is_ok_and(|existing| existing == hlsl) {
        println!("sdf-hlsl: {} is already current", package_gate::HLSL_PATH);
        return;
    }

    if let Err(e) = std::fs::write(&hlsl_path, &hlsl) {
        eprintln!("sdf-hlsl: cannot write {}: {e}", hlsl_path.display());
        std::process::exit(1);
    }
    println!(
        "sdf-hlsl: wrote {} ({} bytes) from {}",
        package_gate::HLSL_PATH,
        hlsl.len(),
        package_gate::WGSL_PATH
    );
}
