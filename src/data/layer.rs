use crate::data::defaults;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Layer<M: Default> {
    #[serde(default = "defaults::r#false")]
    pub launch: bool,
    #[serde(default = "defaults::r#false")]
    pub build: bool,
    #[serde(default = "defaults::r#false")]
    pub cache: bool,
    #[serde(default)]
    pub metadata: M,
}

impl <M: Default> Layer<M> {
    pub fn new() -> Self {
        Layer {
            launch: false,
            build: false,
            cache: false,
            metadata: Default::default()
        }
    }

    /// Reset flags to false and empty metadata table.
    pub fn clear(&mut self) {
        self.launch = false;
        self.build = false;
        self.cache = false;
        self.metadata = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_optional() {
        let layer: Result<Layer<Option<String>>, toml::de::Error> = toml::from_str(
            r#"
            launch = true
            build = true
            cache = false
            "#,
        );

        assert!(!layer.is_err());
    }
}
