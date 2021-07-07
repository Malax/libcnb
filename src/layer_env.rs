use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;

use crate::Env;

/// Provides access to layer environment variables.
#[derive(Eq, PartialEq, Debug)]
pub struct LayerEnv {
    data: Vec<LayerEnvEntry>,
}

#[derive(Eq, PartialEq, Debug)]
struct LayerEnvEntry {
    r#type: LayerEnvEntryType,
    name: OsString,
    value: OsString,
}

#[derive(Eq, PartialEq, Debug)]
enum LayerEnvEntryType {
    Append,
    Default,
    Delimiter,
    Override,
    Prepend,
}

impl LayerEnv {
    pub fn empty() -> Self {
        LayerEnv { data: vec![] }
    }

    pub fn apply(&self, environment: &Env) -> Env {
        let mut environment = environment.clone();

        for layer_environment_variable_entry in &self.data {
            match layer_environment_variable_entry.r#type {
                LayerEnvEntryType::Override => {
                    environment.insert(
                        &layer_environment_variable_entry.name,
                        &layer_environment_variable_entry.value,
                    );
                }
                LayerEnvEntryType::Default => {
                    if !environment.contains_key(&layer_environment_variable_entry.name) {
                        environment.insert(
                            &layer_environment_variable_entry.name,
                            &layer_environment_variable_entry.value,
                        );
                    }
                }
                LayerEnvEntryType::Append => {
                    let mut previous_value = environment
                        .get(&layer_environment_variable_entry.name)
                        .unwrap_or(OsString::new());

                    if previous_value.len() > 0 {
                        previous_value
                            .push(self.delimiter_for(&layer_environment_variable_entry.name));
                    }

                    previous_value.push(&layer_environment_variable_entry.value);

                    environment.insert(&layer_environment_variable_entry.name, previous_value);
                }
                LayerEnvEntryType::Prepend => {
                    let previous_value = environment
                        .get(&layer_environment_variable_entry.name)
                        .unwrap_or(OsString::new());

                    let mut new_value = OsString::new();
                    new_value.push(&layer_environment_variable_entry.value);

                    if !previous_value.is_empty() {
                        new_value.push(self.delimiter_for(&layer_environment_variable_entry.name));
                        new_value.push(previous_value);
                    }

                    environment.insert(&layer_environment_variable_entry.name, new_value);
                }
                _ => (),
            };
        }

        environment
    }

    pub fn insert_override(
        &mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> &Self {
        self.insert(LayerEnvEntryType::Override, name, value)
    }

    pub fn insert_append(
        &mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> &Self {
        self.insert(LayerEnvEntryType::Append, name, value)
    }

    pub fn insert_prepend(
        &mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> &Self {
        self.insert(LayerEnvEntryType::Prepend, name, value)
    }

    pub fn insert_default(
        &mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> &Self {
        self.insert(LayerEnvEntryType::Default, name, value)
    }

    pub fn insert_delimiter(
        &mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> &Self {
        self.insert(LayerEnvEntryType::Delimiter, name, value)
    }

    fn insert(
        &mut self,
        r#type: LayerEnvEntryType,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> &Self {
        let name = name.into();
        let value = value.into();

        let existing_entry_position = self
            .data
            .iter()
            .position(|entry| entry.name == name && entry.r#type == r#type);

        if let Some(existing_entry_position) = existing_entry_position {
            self.data.remove(existing_entry_position);
        }

        self.data.push(LayerEnvEntry {
            r#type,
            name,
            value,
        });

        self
    }

    pub(crate) fn delimiter_for(&self, key: impl AsRef<OsStr>) -> OsString {
        self.data
            .iter()
            .find(|entry| {
                entry.name == key.as_ref() && entry.r#type == LayerEnvEntryType::Delimiter
            })
            .map(|entry| entry.value.clone())
            .unwrap_or(OsString::new())
    }

    pub(crate) fn read_from_env_dir(path: impl AsRef<Path>) -> Result<LayerEnv, std::io::Error> {
        let mut layer_env = LayerEnv::empty();

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
                        Some(LayerEnvEntryType::Override)
                    }
                    Some(file_name_extension) => match file_name_extension.to_str() {
                        Some("append") => Some(LayerEnvEntryType::Append),
                        Some("default") => Some(LayerEnvEntryType::Default),
                        Some("delim") => Some(LayerEnvEntryType::Delimiter),
                        Some("override") => Some(LayerEnvEntryType::Override),
                        Some("prepend") => Some(LayerEnvEntryType::Prepend),
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
}

impl From<Env> for LayerEnv {
    fn from(env: Env) -> Self {
        let mut layer_environment_variables = LayerEnv::empty();

        for (key, value) in &env {
            layer_environment_variables
                .insert_override(key.to_str().unwrap(), value.to_str().unwrap());
        }

        layer_environment_variables
    }
}

impl From<LayerEnv> for Env {
    fn from(layer_env_vars: LayerEnv) -> Self {
        layer_env_vars.apply(&Env::empty())
    }
}

#[cfg(test)]
mod test {
    use crate::Env;
    use crate::LayerEnv;
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

        let layer_env = LayerEnv::read_from_env_dir(temp_dir.path()).unwrap();
        let modified_env = layer_env.apply(&original_env);

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

        let layer_env = LayerEnv::read_from_env_dir(temp_dir.path()).unwrap();
        let modified_env = layer_env.apply(&original_env);

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

    fn environment_as_sorted_vector(environment: &Env) -> Vec<(&str, &str)> {
        let mut result: Vec<(&str, &str)> = environment
            .iter()
            .map(|(k, v)| (k.to_str().unwrap(), v.to_str().unwrap()))
            .collect();

        result.sort_by_key(|kv| kv.0);
        result
    }
}
