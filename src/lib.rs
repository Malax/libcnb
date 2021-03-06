//! This crate provides a library to implement [Cloud Native Buildpacks](https://buildpacks.io/).

pub mod data;
pub mod layer_lifecycle;
pub use build::BuildContext;
pub use detect::DetectContext;
pub use detect::DetectOutcome;
pub use error::*;
pub use generic::*;
pub use platform::*;
pub use runtime::cnb_runtime;
pub use toml_file::*;

mod build;
mod detect;
mod error;
mod generic;
mod platform;
mod runtime;
mod toml_file;
