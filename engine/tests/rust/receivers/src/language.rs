use anyhow::Result;

use crate::model::{Graph, Options, Repo};

pub trait Language {
    fn parse(&self, repo: &Repo, opts: &Options) -> Result<Graph>;
}

pub struct Plain;

impl Language for Plain {
    fn parse(&self, repo: &Repo, opts: &Options) -> Result<Graph> {
        Ok(Graph { names: vec![repo.root.clone()] })
    }
}

pub fn of(repo: &Repo, opts: &Options) -> Result<Box<dyn Language>> {
    Ok(Box::new(Plain))
}
