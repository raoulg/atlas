//! A chart lends another its title and its width, handing each as a field of its own.

pub struct Size {
    pub width: u32,
}

pub struct Chart {
    pub title: String,
    pub size: Size,
}

impl Chart {
    pub fn take(&mut self, title: &str, width: u32) {
        self.title = title.to_string();
        self.size.width = width;
    }

    pub fn lend(&self, other: &mut Chart) {
        other.take(&self.title, self.size.width);
    }
}
