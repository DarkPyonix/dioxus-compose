use dioxus_compose::codegen::{
    EVENT_VECTOR_RELATIVE_PATH, GENERATED_RELATIVE_PATH, MUTATION_VECTOR_RELATIVE_PATH,
    VECTOR_DESCRIPTION_RELATIVE_PATH, generate_event_vector, generate_kotlin,
    generate_mutation_vector, generate_vector_description,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    write(
        manifest_dir.join(GENERATED_RELATIVE_PATH),
        generate_kotlin().as_bytes(),
    )?;
    write(
        manifest_dir.join(MUTATION_VECTOR_RELATIVE_PATH),
        &generate_mutation_vector()?,
    )?;
    write(
        manifest_dir.join(EVENT_VECTOR_RELATIVE_PATH),
        &generate_event_vector()?,
    )?;
    write(
        manifest_dir.join(VECTOR_DESCRIPTION_RELATIVE_PATH),
        generate_vector_description().as_bytes(),
    )?;
    Ok(())
}

fn write(path: PathBuf, contents: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().ok_or("generated output has no parent")?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(&path, contents)?;
    println!("generated {}", path.display());
    Ok(())
}
