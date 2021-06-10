use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

use crate::{data::layer::Layer as ContentMetadata, Error};

/// CNB Layer
pub struct Layer<M: Default + DeserializeOwned + Serialize> {
    pub name: String,
    path: PathBuf,
    content_metadata_path: PathBuf,
    content_metadata: ContentMetadata<M>,
}

impl<M: Default + DeserializeOwned + Serialize> AsRef<Path> for Layer<M> {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}

impl<M: Default + DeserializeOwned + Serialize> Layer<M> {
    /// Layer Constructor that makes a ready to go layer:
    /// * create `/<layers_dir>/<layer> if it doesn't exist
    /// * `/<layers_dir>/<layer>.toml` will be read and parsed from disk if found. If not found an
    /// empty [`crate::data::layer::Layer`] will be constructed.
    ///
    /// # Errors
    /// This function will return an error when:
    /// * if it can not create the layer dir
    /// * if it can not deserialize Layer Content Metadata to [`crate::data::layer::Layer`]
    ///
    /// # Examples
    /// ```
    /// # use tempfile::tempdir;
    /// use libcnb::layer::Layer;
    ///
    /// # fn main() -> Result<(), libcnb::Error> {
    /// # use toml::value::Table;
    /// let layers_dir = tempdir().unwrap().path().to_owned();
    /// let layer = Layer::<Table>::new("foo", layers_dir)?;
    ///
    /// assert!(layer.as_path().exists());
    /// assert_eq!(layer.content_metadata().launch, false);
    /// assert_eq!(layer.content_metadata().build, false);
    /// assert_eq!(layer.content_metadata().cache, false);
    /// assert!(layer.content_metadata().metadata.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(name: impl Into<String>, layers_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let name = name.into();
        let layers_dir = layers_dir.as_ref();
        let path = layers_dir.join(&name);

        fs::create_dir_all(&path)?;

        let content_metadata_path = layers_dir.join(format!("{}.toml", &name));
        let content_metadata = if let Ok(contents) = fs::read_to_string(&content_metadata_path) {
            toml::from_str(&contents)?
        } else {
            ContentMetadata::new()
        };

        Ok(Layer {
            name,
            path,
            content_metadata,
            content_metadata_path,
        })
    }

    /// Layer Constructor that uses [`crate::layer::Layer::new`] and takes a [`std::ops::FnOnce`] to
    /// specifiying Content Metadata and writes it a `<layer>.toml`.
    ///
    /// # Examples
    /// ```
    /// # use tempfile::tempdir;
    /// use libcnb::layer::Layer;
    ///
    /// # fn main() -> Result<(), libcnb::Error> {
    /// # use toml::value::Table;
    /// let layers_dir = tempdir().unwrap().path().to_owned();
    /// let layer = Layer::<Table>::new_with_content_metadata("foo", &layers_dir, |m| {
    ///   m.launch = true;
    ///   m.build = true;
    ///   m.cache = true;
    ///
    ///   m.metadata.insert("foo".to_string(), toml::Value::String("bar".to_string()));
    /// })?;
    ///
    /// assert!(layer.as_path().exists());
    /// assert_eq!(layer.content_metadata().launch, true);
    /// assert_eq!(layer.content_metadata().build, true);
    /// assert_eq!(layer.content_metadata().cache, true);
    /// assert_eq!(layer.content_metadata().metadata.get("foo"),
    /// Some(&toml::Value::String("bar".to_string())));
    /// assert!(&layers_dir.join("foo.toml").exists());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_content_metadata(
        name: impl Into<String>,
        layers_dir: impl AsRef<Path>,
        func: impl FnOnce(&mut ContentMetadata<M>),
    ) -> Result<Self, Error> {
        let mut layer = Self::new(name, layers_dir)?;
        layer.write_content_metadata_with_fn(func)?;

        Ok(layer)
    }

    /// Returns the path to the layer contents `/<layers_dir>/<layer>/`.
    pub fn as_path(&self) -> &Path {
        self.path.as_path()
    }

    /// Returns a reference to the [`crate::data::layer::Layer`]
    pub fn content_metadata(&self) -> &ContentMetadata<M> {
        &self.content_metadata
    }

    #[deprecated(
    since = "0.1.1",
    note = "Please use content_metadata_mut function intsead"
    )]
    /// Returns a mutable reference to the [`crate::data::layer::Layer`]
    pub fn mut_content_metadata(&mut self) -> &mut ContentMetadata<M> {
        self.content_metadata_mut()
    }

    /// Returns a mutable reference to the [`crate::data::layer::Layer`]
    pub fn content_metadata_mut(&mut self) -> &mut ContentMetadata<M> {
        &mut self.content_metadata
    }

    /// Write [`crate::data::layer::Layer`] to `<layer>.toml`
    pub fn write_content_metadata(&self) -> Result<(), crate::Error> {
        fs::write(
            &self.content_metadata_path,
            toml::to_string(&self.content_metadata)?,
        )?;

        Ok(())
    }

    /// Mutate [`crate::layer::ContentMetadata`] and write [`crate::data::layer::Layer`] to
    /// `<layer>.toml`
    ///
    /// # Examples
    /// ```
    /// # use tempfile::tempdir;
    /// use libcnb::layer::Layer;
    ///
    /// # fn main() -> Result<(), libcnb::Error> {
    /// # use toml::value::Table;
    /// let layers_dir = tempdir().unwrap().path().to_owned();
    /// let mut layer = Layer::<Table>::new("foo", &layers_dir)?;
    /// layer.write_content_metadata_with_fn(|m| {
    ///   m.launch = true;
    ///   m.build = true;
    ///   m.cache = true;
    ///
    ///   m.metadata.insert("foo".to_string(), toml::Value::String("bar".to_string()));
    /// })?;
    ///
    /// assert!(layer.as_path().exists());
    /// assert_eq!(layer.content_metadata().launch, true);
    /// assert_eq!(layer.content_metadata().build, true);
    /// assert_eq!(layer.content_metadata().cache, true);
    /// assert_eq!(layer.content_metadata().metadata.get("foo"),
    /// Some(&toml::Value::String("bar".to_string())));
    /// assert!(&layers_dir.join("foo.toml").exists());
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_content_metadata_with_fn(
        &mut self,
        func: impl FnOnce(&mut ContentMetadata<M>),
    ) -> Result<(), crate::Error> {
        func(self.content_metadata_mut());
        self.write_content_metadata()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;
    use toml::value::Table;

    use super::*;

    #[derive(Serialize, Deserialize)]
    struct CustomMetadataStruct {
        bar: String,
    }

    impl Default for CustomMetadataStruct {
        fn default() -> Self {
            CustomMetadataStruct { bar: String::from("Default Value") }
        }
    }

    #[test]
    fn new_reads_layer_toml_metadata() -> Result<(), anyhow::Error> {
        let layers_dir = tempdir()?.path().to_owned();
        fs::create_dir_all(&layers_dir)?;
        fs::write(
            layers_dir.join("foo.toml"),
            r#"
[metadata]
bar = "baz"
"#,
        )?;

        let layer = Layer::<CustomMetadataStruct>::new("foo", &layers_dir)?;
        assert_eq!(layer.content_metadata().metadata.bar, "baz");

        Ok(())
    }

    #[test]
    fn new_reads_layer_toml_metadata_as_table() -> Result<(), anyhow::Error> {
        let layers_dir = tempdir()?.path().to_owned();
        fs::create_dir_all(&layers_dir)?;
        fs::write(
            layers_dir.join("foo.toml"),
            r#"
[metadata]
bar = "baz"
"#,
        )?;

        let layer = Layer::<Table>::new("foo", &layers_dir)?;
        assert_eq!(layer.content_metadata().metadata.get("bar").and_then(|x| x.as_str()).unwrap(), "baz");

        Ok(())
    }
}
