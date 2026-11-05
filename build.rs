use std::{env, fs};

const BUILD_ID_PATH: &str = "build-id.txt";

fn valid_build_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.'))
}

fn main() {
    println!("cargo:rerun-if-changed={BUILD_ID_PATH}");
    println!("cargo:rerun-if-env-changed=RUSTNOTE_BUILD_ID");

    let committed_build_id = fs::read_to_string(BUILD_ID_PATH)
        .unwrap_or_else(|error| panic!("{BUILD_ID_PATH} must exist and be readable: {error}"))
        .trim()
        .to_owned();
    assert!(
        valid_build_id(&committed_build_id),
        "{BUILD_ID_PATH} must contain only ASCII letters, digits, '-' or '.'"
    );

    let build_id = env::var("RUSTNOTE_BUILD_ID")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or(committed_build_id);
    assert!(
        valid_build_id(&build_id),
        "RUSTNOTE_BUILD_ID must contain only ASCII letters, digits, '-' or '.'"
    );
    println!("cargo:rustc-env=RUSTNOTE_BUILD_ID={build_id}");
}
