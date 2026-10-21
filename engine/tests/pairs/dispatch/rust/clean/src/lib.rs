//! Text read from a source, as it comes or trimmed, each method called on the value it runs on.

pub struct Text(pub String);

pub trait Source {
    /// The source's text as it comes.
    fn raw(&self) -> String;

    /// The source's text, as most sources give it.
    fn read(&self) -> Text {
        Text(self.raw())
    }
}

pub struct Plain;

pub struct Trimmed;

impl Source for Plain {
    fn raw(&self) -> String {
        "plain".to_string()
    }
}

impl Source for Trimmed {
    fn raw(&self) -> String {
        " trimmed ".to_string()
    }

    fn read(&self) -> Text {
        Text(self.raw().trim().to_string())
    }
}

/// A plain source's text, by the trait's own `read`.
pub fn plain(p: &Plain) -> Text {
    p.read()
}

/// A trimmed source's text, by its own `read`.
pub fn trimmed(t: &Trimmed) -> Text {
    t.read()
}

/// The text of a source of either kind.
pub fn any(s: &dyn Source) -> Text {
    s.read()
}

/// The texts of a source of each kind.
pub fn both(p: &Plain, t: &Trimmed) -> (Text, Text) {
    (p.read(), t.read())
}
