pub fn post(n: u32) -> u32 {
    n + 1
}

pub fn save(n: u32) {
    println!("{n}");
}

#[doc = "prophecy: supersedes=self::save"]
pub fn save_v3(_n: u32) {
    todo!()
}

#[doc = "prophecy: supersedes=super::shelf::keep"]
pub fn keep_v2(_n: u32) -> u32 {
    todo!()
}
