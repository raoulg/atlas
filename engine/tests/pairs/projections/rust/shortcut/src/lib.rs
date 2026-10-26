//! The same chart, which binds its title and its size to names before it hands them on.

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
        let t = &self.title;
        let s = &self.size;
        other.take(t, s.width);
    }
}
