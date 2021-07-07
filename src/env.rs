use std::collections::HashMap;
use std::env;
use std::env::VarsOs;
use std::ffi::{OsStr, OsString};
/// Generic collection of environment variables.
///
/// # Examples
/// ```
/// use std::process::Command;
/// use libcnb::Env;
///
/// let mut env = Env::empty();
/// env.insert("FOO", "BAR");
/// env.insert("BAZ", "BLAH");
///
/// let output = Command::new("printenv")
///     .env_clear()
///     .envs(&env)
///     .output()
///     .unwrap();
///
/// assert_eq!(
///     "BAZ=BLAH\nFOO=BAR\n",
///     String::from_utf8_lossy(&output.stdout)
/// );
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Env {
    inner: HashMap<OsString, OsString>,
}

impl Env {
    /// Creates a new `Env` from all the environment variables of the current process.
    ///
    /// The returned `Env` contains a snapshot of the process's environment
    /// variables at the time of this invocation. Modifications to environment
    /// variables afterwards will not be reflected in the returned value.
    ///
    /// See [`std::env::vars_os`]
    pub fn from_current() -> Self {
        env::vars_os().into()
    }

    /// Creates an empty `Environment` struct.
    pub fn empty() -> Self {
        Env {
            inner: HashMap::new(),
        }
    }

    /// Inserts a key-value pair into the environment, overriding the value if `key` was already
    /// present.
    pub fn insert(&mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> &mut Self {
        self.inner.insert(key.into(), value.into());
        self
    }

    /// Returns a cloned value corresponding to the given key.
    pub fn get<T: From<OsString>>(&self, key: impl AsRef<OsStr>) -> Option<T> {
        self.inner
            .get(key.as_ref())
            .map(|value| T::from(value.clone()))
    }

    /// Returns true if the environment contains a value for the specified key.
    pub fn contains_key(&self, key: impl AsRef<OsStr>) -> bool {
        self.inner.contains_key(key.as_ref())
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, OsString, OsString> {
        self.inner.iter()
    }
}

impl From<VarsOs> for Env {
    fn from(vars_os: VarsOs) -> Self {
        Env {
            inner: vars_os.collect(),
        }
    }
}

impl<'a> IntoIterator for &'a Env {
    type Item = (&'a OsString, &'a OsString);
    type IntoIter = std::collections::hash_map::Iter<'a, OsString, OsString>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

mod test {
    #[test]
    #[cfg(target_family = "unix")]
    fn test_into_iterator() {
        use crate::Env;
        use std::process::Command;

        let mut env = Env::empty();
        env.insert("FOO", "FOO");
        env.insert("FOO", "BAR");
        env.insert("BAZ", "BLAH");

        let output = Command::new("printenv")
            .env_clear()
            .envs(&env)
            .output()
            .unwrap();

        assert_eq!(
            "BAZ=BLAH\nFOO=BAR\n",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
