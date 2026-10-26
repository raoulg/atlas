pub struct Name(pub String);
pub struct Key(pub u64);

pub trait Named {
    fn id(&self) -> Name;
}

pub trait Keyed {
    fn id(&self) -> Key;
}

pub struct Book;

impl Keyed for Book {
    fn id(&self) -> Key {
        Key(1)
    }
}

impl Named for Book {
    fn id(&self) -> Name {
        Name("book".to_string())
    }
}
