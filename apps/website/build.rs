// generated by `sqlx migrate build-script`
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // trigger recompilation when a new migration is added
    println!("cargo:rerun-if-changed=migrations");
    tonic_build::compile_protos("../../protos/build_service.proto")?;
    Ok(())
}
