//! Sample model type with methods.

/// A person in the sample domain.
pub struct Person {
    pub name: String,
}

impl Person {
    /// Builds a person with one required field.
    pub fn new(name: &str) -> Self {
        Person { name: name.to_string() }
    }

    /// Renames in place (mutating method sample).
    pub fn rename(&mut self, name: &str) {
        self.name = name.to_string();
    }
}
