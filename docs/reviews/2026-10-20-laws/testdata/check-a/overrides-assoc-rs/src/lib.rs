pub struct Book;

pub trait Store {
    type Item;
    fn get(&self) -> Self::Item;
}

pub struct Shelf;

impl Store for Shelf {
    type Item = Book;
    fn get(&self) -> Book {
        Book
    }
}
