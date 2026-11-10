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

/// Where a board's CUDA target directory lives, on the host and inside the cross container.
pub struct CudaProfile {
    /// Default host path for the CUDA target directory.
    pub host_path: &'static str,
    /// Path inside the cross container where CUDA is expected (fixed by the Dockerfile ENV vars).
    pub container_path: &'static str,
}

pub struct BoardProfile {
    /// Rust/cross target triple (e.g. `aarch64-unknown-linux-gnu`).
    pub cross_triple: &'static str,
    /// CUDA mount for GPU boards; `None` for boards without CUDA (e.g. RISC-V).
    pub cuda: Option<CudaProfile>,
    /// Default `--device` for the AOT kernel compile step.
    pub device: &'static str,
    /// Default `--options` for the AOT kernel compile step, used when `--device` is the default.
    pub options: &'static str,
    /// `qemu-riscv64 -cpu` model for running the cross-built binary under user-mode emulation;
    /// `None` for boards `cargo teeny run` doesn't support.
    pub qemu_cpu: Option<&'static str>,
}

pub fn board_profile(board: BoardType) -> BoardProfile {
    match board {
        BoardType::JetsonOrinNano => BoardProfile {
            cross_triple: "aarch64-unknown-linux-gnu",
            cuda: Some(CudaProfile {
                host_path: "/usr/local/cuda-12.6/targets/aarch64-linux",
                container_path: "/usr/local/cuda-12.6/targets/aarch64-linux",
            }),
            device: "cuda",
            options: "capability=sm_87",
            qemu_cpu: None,
        },
        // The host binary is plain RV64GC (no `+v`) so it runs on any RISC-V Linux board; only
        // the AOT-compiled kernels use RVV, selected via the `capability` option.
        BoardType::RiscvRvv => BoardProfile {
            cross_triple: "riscv64gc-unknown-linux-gnu",
            cuda: None,
            device: "riscv",
            options: "capability=generic-rvv1.0",
            // `max` enables every extension QEMU emulates, including V (matches teeny-riscv).
            qemu_cpu: Some("max"),
        },
        BoardType::SpacemitK3 => BoardProfile {
            cross_triple: "riscv64gc-unknown-linux-gnu",
            cuda: None,
            device: "riscv",
            options: "capability=spacemit-k3",
            qemu_cpu: Some("max"),
        },
    }
}
