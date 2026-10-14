//! A knowledge base kept in a file, a node per line; read back as the graph of its nodes.

use std::fs;
use std::io;
use std::path::PathBuf;

pub struct Begrip {
    pub uuid: String,
    pub naam: String,
}

pub struct Bron {
    pub uuid: String,
    pub url: String,
}

pub enum Node {
    Begrip(Begrip),
    Bron(Bron),
}

pub struct Graaf {
    pub nodes: Vec<Node>,
}

pub struct Kennisbank {
    path: PathBuf,
}

impl Kennisbank {
    pub fn new(path: PathBuf) -> Kennisbank {
        Kennisbank { path }
    }

    pub fn voeg_toe(&self, bron: &Bron) -> io::Result<()> {
        self.write(&format!("{} bron {}", bron.uuid, bron.url))
    }

    pub fn graaf(&self) -> io::Result<Graaf> {
        Ok(Graaf { nodes: fs::read_to_string(&self.path)?.lines().map(node).collect() })
    }

    fn write(&self, line: &str) -> io::Result<()> {
        let mut text = fs::read_to_string(&self.path)?;
        text.push_str(line);
        text.push('\n');
        fs::write(&self.path, text)
    }
}

fn node(line: &str) -> Node {
    let mut parts = line.splitn(3, ' ');
    let (uuid, soort, tekst) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if soort == "bron" {
        Node::Bron(Bron { uuid: uuid.to_string(), url: tekst.to_string() })
    } else {
        Node::Begrip(Begrip { uuid: uuid.to_string(), naam: tekst.to_string() })
    }
}
