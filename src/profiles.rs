use clap::ValueEnum;

use crate::cli::BoardType;

/// The kebab-case CLI name for a board (e.g. `jetson-orin-nano`), as used in
/// `--target`/`--type` and recorded in marker files.
pub fn board_type_cli_name(board: BoardType) -> String {
    board
        .to_possible_value()
        .expect("BoardType maps to a clap PossibleValue")
        .get_name()
        .to_owned()
}

pub struct BoardProfile {
    /// Rust/cross target triple (e.g. `aarch64-unknown-linux-gnu`).
    pub cross_triple: &'static str,
    /// Default host path for the CUDA aarch64 target directory.
    pub cuda_host_path: &'static str,
    /// Path inside the cross container where CUDA is expected (fixed by the Dockerfile ENV vars).
    pub cuda_container_path: &'static str,
}

pub fn board_profile(board: BoardType) -> BoardProfile {
    match board {
        BoardType::JetsonOrinNano => BoardProfile {
            cross_triple: "aarch64-unknown-linux-gnu",
            cuda_host_path: "/usr/local/cuda-12.6/targets/aarch64-linux",
            cuda_container_path: "/usr/local/cuda-12.6/targets/aarch64-linux",
        },
    }
}
