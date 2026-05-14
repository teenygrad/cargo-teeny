use crate::cli::BoardType;

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
