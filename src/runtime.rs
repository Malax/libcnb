use std::{env, fs, path::PathBuf, process};

use libcnb::build::BuildContext;
use libcnb::platform::Platform;

use crate::build::{cnb_runtime_build, BuildContext};
use crate::detect::{cnb_runtime_detect, DetectContext, DetectOutcome};
use crate::{
    data::{buildpack::BuildpackToml, buildpack_plan::BuildpackPlan, launch::Launch},
    layer::Layer,
    platform::{GenericPlatform, Platform},
    shared::read_toml_file,
    Error,
};
use std::process::exit;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn cnb_runtime<
    P: Platform,
    DF: Fn(DetectContext<P>) -> Result<DetectOutcome, E>,
    BF: Fn(BuildContext<P>) -> Result<(), E>,
    E: std::fmt::Display,
>(
    detect_fn: DF,
    build_fn: BF,
) {
    let current_exe_file_name = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name());

    match current_exe_file_name {
        Option("detect") => cnb_runtime_detect(detect_fn),
        Option("build") => cnb_runtime_build(build_fn),
        Option(_) | None => exit(255),
    }
}
