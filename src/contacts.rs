use std::{collections::HashSet, hash::Hash, path::Path};

use serde::Deserialize;

#[derive(Debug)]
pub enum Error {
    #[allow(dead_code)] // Not dead, used to fmt error logs etc via "{:?}"
    Reader(csv::Error),
}
impl From<csv::Error> for Error {
    fn from(value: csv::Error) -> Self {
        Self::Reader(value)
    }
}

#[derive(Debug, Deserialize)]
pub struct Contact {
    name: String,
    number: String,
}
impl Contact {
    pub fn number(&self) -> &str {
        &self.number
    }
}
impl PartialEq for Contact {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl Eq for Contact {}
impl Hash for Contact {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

#[derive(Debug, Default)]
pub struct ContactList(HashSet<Contact>);

impl ContactList {
    const DEFAULT_PATH: &str = "./contacts.csv";

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut rdr = csv::Reader::from_path(path)?;
        let mut hashset: HashSet<Contact> = HashSet::new();

        for record in rdr.deserialize() {
            hashset.insert(record?);
        }

        Ok(Self(hashset))
    }

    pub fn from_default() -> Self {
        Self::from_path(Self::DEFAULT_PATH)
            .inspect_err(|err| error!("Unable to load default contact list: {:?}", err))
            .unwrap_or_default()
    }

    pub fn find(&self, name: &str) -> Option<&Contact> {
        let search_value = Contact {
            name: name.to_lowercase(),
            number: Default::default(),
        };
        self.0.get(&search_value)
    }

    pub fn list(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|contact| contact.name.to_string())
            .collect()
    }
}
