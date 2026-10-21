pub struct Book;
pub struct Name(pub String);
pub struct Key(pub u64);

pub trait Store {
    type Item;
    fn get(&self) -> Self::Item;
}

pub trait Named {
    fn id(&self) -> Name;
}

pub trait Keyed {
    fn id(&self) -> Key;
}

pub struct Shelf;

impl Store for Shelf {
    type Item = Book;
    fn get(&self) -> Book {
        Book
    }
}

impl Named for Book {
    fn id(&self) -> Name {
        Name("book".to_string())
    }
}

impl Keyed for Book {
    fn id(&self) -> Key {
        Key(1)
    }
}
