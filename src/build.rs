use crate::{
    data::{buildpack::BuildpackToml, buildpack_plan::BuildpackPlan, launch::Launch},
    layer::Layer,
    platform::{GenericPlatform, Platform},
    shared::read_toml_file,
    Error,
};
use std::{env, fs, path::PathBuf, process};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub fn cnb_runtime_build<
    E: std::fmt::Display,
    F: Fn(BuildContext<P>) -> Result<(), E>,
    P: Platform,
>(
    build_fn: F,
) {
    let app_dir = env::current_dir().expect("Could not determine current working directory!");

    let buildpack_dir: PathBuf = env::var("CNB_BUILDPACK_DIR")
        .expect("Could not determine buildpack directory!")
        .into();

    let stack_id: String = env::var("CNB_STACK_ID")
        .expect("Could not determine CNB stack id!")
        .into();

    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: build <layers> <platform> <plan>");
        process::exit(1);
    }

    let layers_dir: PathBuf = args.get(1).unwrap().into();

    let platform = {
        let platform_dir = PathBuf::from(args.get(2).unwrap());

        if !platform_dir.is_dir() {
            eprintln!("Second argument must be a readable platform directory!");
            process::exit(1);
        }

        match P::from_path(platform_dir.as_path()) {
            Ok(platform) => platform,
            Err(error) => {
                eprintln!(
                    "Could not create platform from platform directory: {}",
                    error
                );
                process::exit(1);
            }
        }
    };

    let buildpack_plan = {
        let buildpack_plan_path: PathBuf = args.get(3).unwrap().into();
        match read_toml_file(&buildpack_plan_path) {
            Ok(buildpack_plan) => buildpack_plan,
            Err(error) => {
                eprintln!("Could not read buildpack plan: {}", error);
                process::exit(1);
            }
        }
    };

    let buildpack_toml_path = buildpack_dir.join("buildpack.toml");
    let buildpack_descriptor = match read_toml_file(buildpack_toml_path) {
        Ok(buildpack_descriptor) => buildpack_descriptor,
        Err(error) => {
            eprintln!("Could not read buildpack descriptor: {}", error);
            process::exit(1);
        }
    };

    let context = BuildContext {
        layers_dir,
        app_dir,
        buildpack_dir,
        stack_id,
        platform,
        buildpack_plan,
        buildpack_descriptor,
    };

    match build_fn(context) {
        Err(error) => {
            eprintln!("Unhandled error during build: {}", error);
            process::exit(-1);
        }
        _ => process::exit(0),
    };
}

pub struct BuildContext<P: Platform> {
    layers_dir: PathBuf,
    pub app_dir: PathBuf,
    pub buildpack_dir: PathBuf,
    pub stack_id: String,
    pub platform: P,
    pub buildpack_plan: BuildpackPlan,
    pub buildpack_descriptor: BuildpackToml,
}

impl<P: Platform> BuildContext<P> {
    /// Get access to a new or existing layer
    pub fn layer<M: Default + DeserializeOwned + Serialize>(&self, name: impl AsRef<str>) -> Result<Layer<M>, Error> {
        Layer::new(name.as_ref(), self.layers_dir.as_path())
    }

    pub fn write_launch(&self, data: Launch) -> Result<(), Error> {
        let path = self.layers_dir.join("launch.toml");
        fs::write(path, toml::to_string(&data)?)?;

        Ok(())
    }
}

pub type GenericBuildContext = BuildContext<GenericPlatform>;
