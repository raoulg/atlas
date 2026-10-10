//! Functions handed on as values, to be called by whoever receives them.

pub struct Item {
    pub n: u32,
}

fn double(x: &Item) -> u32 {
    x.n * 2
}

impl Item {
    fn label(&self) -> String {
        format!("{}", self.n)
    }

    pub fn labels(items: &[Item]) -> Vec<String> {
        items.iter().map(Self::label).collect()
    }
}

pub fn doubled(items: &[Item]) -> Vec<u32> {
    items.iter().map(double).collect()
}
