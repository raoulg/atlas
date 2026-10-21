pub struct Runner {
    pub mode: String,
}

impl Runner {
    pub fn size(&self) -> usize {
        let a = if self.mode == "parallel" { 64 } else { 1 };
        a + if self.mode == "parallel" { 8 } else { 1 }
    }
}

pub fn size(mode: &str) -> usize {
    let a = if mode == "parallel" { 64 } else { 1 };
    a + if mode == "parallel" { 8 } else { 1 }
}
