use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;

use crate::Env;
use std::collections::hash_map::Entry;
use std::collections::HashMap;

/// Provides access to layer environment variables.
#[derive(Eq, PartialEq, Debug)]
pub struct LayerEnv {
    all: LayerEnvDelta,
    build: LayerEnvDelta,
    launch: LayerEnvDelta,
    process: HashMap<String, LayerEnvDelta>,

    // Entries for the standard layer paths as described in the CNB spec:
    // https://github.com/buildpacks/spec/blob/a9f64de9c78022aa7a5091077a765f932d7afe42/buildpack.md#layer-paths
    // These cannot be set by the user itself and are only populated when a `LayerEnv` is read from
    // disk by this library.
    layer_paths: LayerEnvDelta,
}

#[derive(Eq, PartialEq, Debug)]
struct LayerEnvDelta {
    entries: Vec<LayerEnvDeltaEntry>,
}

impl LayerEnvDelta {
    fn empty() -> LayerEnvDelta {
        LayerEnvDelta { entries: vec![] }
    }

    fn apply(&self, env: &Env) -> Env {
        let mut result_env = env.clone();

        for entry in &self.entries {
            match entry.modification_behavior {
                ModificationBehavior::Override => {
                    result_env.insert(&entry.name, &entry.value);
                }
                ModificationBehavior::Default => {
                    if !result_env.contains_key(&entry.name) {
                        result_env.insert(&entry.name, &entry.value);
                    }
                }
                ModificationBehavior::Append => {
                    let mut previous_value = result_env.get(&entry.name).unwrap_or(OsString::new());

                    if previous_value.len() > 0 {
                        previous_value.push(self.delimiter_for(&entry.name));
                    }

                    previous_value.push(&entry.value);

                    result_env.insert(&entry.name, previous_value);
                }
                ModificationBehavior::Prepend => {
                    let previous_value = result_env.get(&entry.name).unwrap_or(OsString::new());

                    let mut new_value = OsString::new();
                    new_value.push(&entry.value);

                    if !previous_value.is_empty() {
                        new_value.push(self.delimiter_for(&entry.name));
                        new_value.push(previous_value);
                    }

                    result_env.insert(&entry.name, new_value);
                }
                _ => (),
            };
        }

        result_env
    }

    fn delimiter_for(&self, key: impl AsRef<OsStr>) -> OsString {
        self.entries
            .iter()
            .find(|entry| {
                entry.name == key.as_ref()
                    && entry.modification_behavior == ModificationBehavior::Delimiter
            })
            .map(|entry| entry.value.clone())
            .unwrap_or(OsString::new())
    }

    fn read_from_env_dir(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let mut layer_env = Self::empty();

        for dir_entry in fs::read_dir(path.as_ref())? {
            let path = dir_entry?.path();

            // Rely on the Rust standard library for splitting stem and extension. Since paths
            // are not necessarily UTF-8 encoded, this is not as trivial as it might look like.
            // Think twice before changing this.
            let file_name_stem = path.file_stem();
            let file_name_extension = path.extension();

            // The CNB spec explicitly states:
            //
            // > File contents MUST NOT be evaluated by a shell or otherwise modified before
            // > inclusion in environment variable values.
            // > https://github.com/buildpacks/spec/blob/a9f64de9c78022aa7a5091077a765f932d7afe42/buildpack.md#provided-by-the-buildpacks
            //
            // This should include parsing the contents with an assumed charset and later emitting
            // the raw bytes of that encoding as it might change the actual data. Since this is not
            // explicitly written in the spec, we read through the the reference implementation and
            // determined that it also treats the file contents as raw bytes.
            // See: https://github.com/buildpacks/lifecycle/blob/a7428a55c2a14d8a37e84285b95dc63192e3264e/env/env.go#L73-L106
            use std::os::unix::ffi::OsStringExt;
            let file_contents = OsString::from_vec(fs::read(&path)?);

            if let Some(file_name_stem) = file_name_stem {
                let r#type = match file_name_extension {
                    None => {
                        // TODO: This is different for CNB API versions > 0.5:
                        // https://github.com/buildpacks/lifecycle/blob/a7428a55c2a14d8a37e84285b95dc63192e3264e/env/env.go#L66-L71
                        Some(ModificationBehavior::Override)
                    }
                    Some(file_name_extension) => match file_name_extension.to_str() {
                        Some("append") => Some(ModificationBehavior::Append),
                        Some("default") => Some(ModificationBehavior::Default),
                        Some("delim") => Some(ModificationBehavior::Delimiter),
                        Some("override") => Some(ModificationBehavior::Override),
                        Some("prepend") => Some(ModificationBehavior::Prepend),
                        // Note: This IS NOT the case where we have no extension. This handles
                        // the case of an unknown or non-UTF-8 extension.
                        Some(_) | None => None,
                    },
                };

                if let Some(r#type) = r#type {
                    layer_env.insert(r#type, file_name_stem.to_os_string(), file_contents);
                }
            }
        }

        Ok(layer_env)
    }

    fn write_to_env_dir(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        fs::remove_dir_all(path.as_ref())?;
        fs::create_dir_all(path.as_ref())?;

        for entry in &self.entries {
            let file_extension = match entry.modification_behavior {
                ModificationBehavior::Append => ".append",
                ModificationBehavior::Default => ".default",
                ModificationBehavior::Delimiter => ".delimiter",
                ModificationBehavior::Override => ".override",
                ModificationBehavior::Prepend => ".prepend",
            };

            let mut file_name = entry.name.clone();
            file_name.push(file_extension);

            let file_path = path.as_ref().join(file_name);

            use std::os::unix::ffi::OsStrExt;
            fs::write(file_path, &entry.value.as_bytes())?;
        }

        Ok(())
    }

    fn insert(
        &mut self,
        modification_behavior: ModificationBehavior,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> &Self {
        let name = name.into();
        let value = value.into();

        let existing_entry_position = self.entries.iter().position(|entry| {
            entry.name == name && entry.modification_behavior == modification_behavior
        });

        if let Some(existing_entry_position) = existing_entry_position {
            self.entries.remove(existing_entry_position);
        }

        self.entries.push(LayerEnvDeltaEntry {
            modification_behavior,
            name,
            value,
        });

        self
    }
}

#[derive(Eq, PartialEq, Debug)]
struct LayerEnvDeltaEntry {
    modification_behavior: ModificationBehavior,
    name: OsString,
    value: OsString,
}

#[derive(Eq, PartialEq, Debug)]
pub enum ModificationBehavior {
    Append,
    Default,
    Delimiter,
    Override,
    Prepend,
}

#[derive(Eq, PartialEq, Debug)]
pub enum TargetLifecycle {
    All,
    Build,
    Launch,
    Process(String),
}

impl LayerEnv {
    pub fn empty() -> Self {
        LayerEnv {
            all: LayerEnvDelta::empty(),
            build: LayerEnvDelta::empty(),
            launch: LayerEnvDelta::empty(),
            process: HashMap::new(),
            layer_paths: LayerEnvDelta::empty(),
        }
    }

    pub fn apply_for_build(&self, env: &Env) -> Env {
        vec![&self.layer_paths, &self.all, &self.build]
            .iter()
            .fold(env.clone(), |env, delta| delta.apply(&env))
    }

    pub fn insert(
        &mut self,
        target: TargetLifecycle,
        modification_behavior: ModificationBehavior,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) {
        let target_delta = match target {
            TargetLifecycle::All => &mut self.all,
            TargetLifecycle::Build => &mut self.build,
            TargetLifecycle::Launch => &mut self.launch,
            TargetLifecycle::Process(process_type_name) => {
                match self.process.entry(process_type_name) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => entry.insert(LayerEnvDelta::empty()),
                }
            }
        };

        target_delta.insert(modification_behavior, name, value);
    }

    pub(crate) fn read_from_layer_dir(path: impl AsRef<Path>) -> Result<LayerEnv, std::io::Error> {
        let bin_path = path.as_ref().join("bin");
        let lib_path = path.as_ref().join("lib");

        let mut layer_path_delta = LayerEnvDelta::empty();
        if bin_path.is_dir() {
            layer_path_delta.insert(ModificationBehavior::Prepend, "PATH", &bin_path);
            layer_path_delta.insert(ModificationBehavior::Delimiter, "PATH", PATH_LIST_SEPARATOR);
        }

        if lib_path.is_dir() {
            layer_path_delta.insert(ModificationBehavior::Prepend, "LIBRARY_PATH", &lib_path);
            layer_path_delta.insert(
                ModificationBehavior::Delimiter,
                "LIBRARY_PATH",
                PATH_LIST_SEPARATOR,
            );

            layer_path_delta.insert(ModificationBehavior::Prepend, "LD_LIBRARY_PATH", &lib_path);
            layer_path_delta.insert(
                ModificationBehavior::Delimiter,
                "LD_LIBRARY_PATH",
                PATH_LIST_SEPARATOR,
            );
        }

        let mut layer_env = LayerEnv::empty();
        layer_env.layer_paths = layer_path_delta;

        let env_path = path.as_ref().join("env");
        if env_path.is_dir() {
            layer_env.all = LayerEnvDelta::read_from_env_dir(env_path)?;
        }

        let env_build_path = path.as_ref().join("env.build");
        if env_build_path.is_dir() {
            layer_env.build = LayerEnvDelta::read_from_env_dir(env_build_path)?;
        }

        let env_launch_path = path.as_ref().join("env.launch");
        if env_launch_path.is_dir() {
            layer_env.launch = LayerEnvDelta::read_from_env_dir(env_launch_path)?;
        }

        Ok(layer_env)
    }
}

#[cfg(test)]
mod test {
    use super::LayerEnvDelta;
    use crate::layer_env::{Env, LayerEnv, ModificationBehavior, TargetLifecycle};
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    /// Direct port of a test from the reference lifecycle implementation:
    /// See: https://github.com/buildpacks/lifecycle/blob/a7428a55c2a14d8a37e84285b95dc63192e3264e/env/env_test.go#L105-L154
    #[test]
    fn test_reference_impl_env_files_have_a_suffix_it_performs_the_matching_action() {
        let temp_dir = tempdir().unwrap();

        let mut files = HashMap::new();
        files.insert("VAR_APPEND.append", "value-append");
        files.insert("VAR_APPEND_NEW.append", "value-append");
        files.insert("VAR_APPEND_DELIM.append", "value-append-delim");
        files.insert("VAR_APPEND_DELIM_NEW.append", "value-append-delim");
        files.insert("VAR_APPEND_DELIM.delim", "[]");
        files.insert("VAR_APPEND_DELIM_NEW.delim", "[]");

        files.insert("VAR_PREPEND.prepend", "value-prepend");
        files.insert("VAR_PREPEND_NEW.prepend", "value-prepend");
        files.insert("VAR_PREPEND_DELIM.prepend", "value-prepend-delim");
        files.insert("VAR_PREPEND_DELIM_NEW.prepend", "value-prepend-delim");
        files.insert("VAR_PREPEND_DELIM.delim", "[]");
        files.insert("VAR_PREPEND_DELIM_NEW.delim", "[]");

        files.insert("VAR_DEFAULT.default", "value-default");
        files.insert("VAR_DEFAULT_NEW.default", "value-default");

        files.insert("VAR_OVERRIDE.override", "value-override");
        files.insert("VAR_OVERRIDE_NEW.override", "value-override");

        files.insert("VAR_IGNORE.ignore", "value-ignore");

        for (file_name, file_contents) in files {
            fs::write(temp_dir.path().join(file_name), file_contents).unwrap();
        }

        let mut original_env = Env::empty();
        original_env.insert("VAR_APPEND", "value-append-orig");
        original_env.insert("VAR_APPEND_DELIM", "value-append-delim-orig");
        original_env.insert("VAR_PREPEND", "value-prepend-orig");
        original_env.insert("VAR_PREPEND_DELIM", "value-prepend-delim-orig");
        original_env.insert("VAR_DEFAULT", "value-default-orig");
        original_env.insert("VAR_OVERRIDE", "value-override-orig");

        let layer_env_delta = LayerEnvDelta::read_from_env_dir(temp_dir.path()).unwrap();
        let modified_env = layer_env_delta.apply(&original_env);

        assert_eq!(
            vec![
                ("VAR_APPEND", "value-append-origvalue-append"),
                (
                    "VAR_APPEND_DELIM",
                    "value-append-delim-orig[]value-append-delim"
                ),
                ("VAR_APPEND_DELIM_NEW", "value-append-delim"),
                ("VAR_APPEND_NEW", "value-append"),
                ("VAR_DEFAULT", "value-default-orig"),
                ("VAR_DEFAULT_NEW", "value-default"),
                ("VAR_OVERRIDE", "value-override"),
                ("VAR_OVERRIDE_NEW", "value-override"),
                ("VAR_PREPEND", "value-prependvalue-prepend-orig"),
                (
                    "VAR_PREPEND_DELIM",
                    "value-prepend-delim[]value-prepend-delim-orig"
                ),
                ("VAR_PREPEND_DELIM_NEW", "value-prepend-delim"),
                ("VAR_PREPEND_NEW", "value-prepend"),
            ],
            environment_as_sorted_vector(&modified_env)
        );
    }

    /// Direct port of a test from the reference lifecycle implementation:
    /// See: https://github.com/buildpacks/lifecycle/blob/a7428a55c2a14d8a37e84285b95dc63192e3264e/env/env_test.go#L188-L210
    #[test]
    fn test_reference_impl_env_files_have_no_suffix_default_action_is_override() {
        let temp_dir = tempdir().unwrap();

        let mut files = HashMap::new();
        files.insert("VAR_NORMAL", "value-normal");
        files.insert("VAR_NORMAL_NEW", "value-normal");
        files.insert("VAR_NORMAL_DELIM", "value-normal-delim");
        files.insert("VAR_NORMAL_DELIM_NEW", "value-normal-delim");
        files.insert("VAR_NORMAL_DELIM.delim", "[]");
        files.insert("VAR_NORMAL_DELIM_NEW.delim", "[]");

        for (file_name, file_contents) in files {
            fs::write(temp_dir.path().join(file_name), file_contents).unwrap();
        }

        let mut original_env = Env::empty();
        original_env.insert("VAR_NORMAL", "value-normal-orig");
        original_env.insert("VAR_NORMAL_DELIM", "value-normal-delim-orig");

        let layer_env_delta = LayerEnvDelta::read_from_env_dir(temp_dir.path()).unwrap();
        let modified_env = layer_env_delta.apply(&original_env);

        assert_eq!(
            vec![
                ("VAR_NORMAL", "value-normal"),
                ("VAR_NORMAL_DELIM", "value-normal-delim"),
                ("VAR_NORMAL_DELIM_NEW", "value-normal-delim"),
                ("VAR_NORMAL_NEW", "value-normal"),
            ],
            environment_as_sorted_vector(&modified_env)
        );
    }

    /// Direct port of a test from the reference lifecycle implementation:
    /// See: https://github.com/buildpacks/lifecycle/blob/a7428a55c2a14d8a37e84285b95dc63192e3264e/env/env_test.go#L55-L80
    #[test]
    fn test_reference_impl_add_root_dir_should_append_posix_directories() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir_all(temp_dir.path().join("bin")).unwrap();
        fs::create_dir_all(temp_dir.path().join("lib")).unwrap();

        let mut original_env = Env::empty();
        original_env.insert("PATH", "some");
        original_env.insert("LD_LIBRARY_PATH", "some-ld");
        original_env.insert("LIBRARY_PATH", "some-library");

        let layer_env = LayerEnv::read_from_layer_dir(temp_dir.path()).unwrap();
        let modified_env = layer_env.apply_for_build(&original_env);

        assert_eq!(
            vec![
                (
                    "LD_LIBRARY_PATH",
                    format!("{}:some-ld", temp_dir.path().join("lib").to_str().unwrap()).as_str()
                ),
                (
                    "LIBRARY_PATH",
                    format!(
                        "{}:some-library",
                        temp_dir.path().join("lib").to_str().unwrap()
                    )
                    .as_str()
                ),
                (
                    "PATH",
                    format!("{}:some", temp_dir.path().join("bin").to_str().unwrap()).as_str()
                )
            ],
            environment_as_sorted_vector(&modified_env)
        );
    }

    #[test]
    fn test_layer_env_delta_fs_read_write() {
        let mut original_delta = LayerEnvDelta::empty();
        original_delta.insert(ModificationBehavior::Default, "FOO", "BAR");
        original_delta.insert(ModificationBehavior::Append, "APPEND_TO_ME", "NEW_VALUE");

        let temp_dir = tempdir().unwrap();

        original_delta.write_to_env_dir(&temp_dir.path()).unwrap();
        let disk_delta = LayerEnvDelta::read_from_env_dir(&temp_dir.path()).unwrap();

        assert_eq!(original_delta, disk_delta);
    }

    #[test]
    fn test_layer_env_insert() {
        let mut layer_env = LayerEnv::empty();
        layer_env.insert(
            TargetLifecycle::Build,
            ModificationBehavior::Append,
            "MAVEN_OPTS",
            "-Dskip.tests=true",
        );

        layer_env.insert(
            TargetLifecycle::All,
            ModificationBehavior::Override,
            "JAVA_TOOL_OPTIONS",
            "-Xmx1G",
        );

        layer_env.insert(
            TargetLifecycle::Build,
            ModificationBehavior::Override,
            "JAVA_TOOL_OPTIONS",
            "-Xmx2G",
        );

        layer_env.insert(
            TargetLifecycle::Launch,
            ModificationBehavior::Append,
            "JAVA_TOOL_OPTIONS",
            "-XX:+UseSerialGC",
        );

        let result_env = layer_env.apply_for_build(&Env::empty());
        assert_eq!(
            vec![
                ("JAVA_TOOL_OPTIONS", "-Xmx2G"),
                ("MAVEN_OPTS", "-Dskip.tests=true")
            ],
            environment_as_sorted_vector(&result_env)
        );
    }

    fn environment_as_sorted_vector(environment: &Env) -> Vec<(&str, &str)> {
        let mut result: Vec<(&str, &str)> = environment
            .iter()
            .map(|(k, v)| (k.to_str().unwrap(), v.to_str().unwrap()))
            .collect();

        result.sort_by_key(|kv| kv.0);
        result
    }
}

#[cfg(target_family = "unix")]
const PATH_LIST_SEPARATOR: &str = ":";

#[cfg(target_family = "windows")]
const PATH_LIST_SEPARATOR: &str = ";";
