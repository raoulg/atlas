//! What a club keeps: its members, their visits and the reports they write.

pub mod desk;
pub mod files;

pub struct Member {
    pub name: String,
}

pub struct Visit {
    pub member: String,
    pub day: String,
}

pub struct Report {
    pub title: String,
    pub author: Member,
}
