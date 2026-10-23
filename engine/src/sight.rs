//! What a picture has in sight: the fibers it keeps and the boxes it opens (`design.md` §1, "A
//! view is data"). The fibers are the route, the kind of route, the part and the packages in focus;
//! the boxes opened are an order ideal of each forest the code is read into (parts into packages,
//! a route's steps into their bodies, families into their members, a place into the places it
//! stands for, a directory into its files). The page applies the fibers first, then the quotients
//! by the ideals' cuts (`view`), and writes its sight to its address after every drawing, so that a
//! picture is a link; `atlas view --view`, `atlas mcp`'s `view` and `look` hand a page the sight an
//! agent wrote.
//!
//! Its serde form is the page's: `{route, kind, part, focus, open: {parts, units, steps,
//! functions, families, places, dirs}}`, every member optional, an empty one left out. A member the
//! page does not know is refused, so that a sight misspelt is said, not drawn as no sight at all.

use std::collections::BTreeSet;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::model::EntryKind;

/// The fibers a picture keeps and the boxes it opens.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sight {
    /// One route, by its entry's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// Every route of one kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntryKind>,
    /// One part, by its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// The packages in focus, by their ids.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub focus: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Opened::is_empty")]
    pub open: Opened,
}

/// The boxes opened, in each forest: a set closed downwards, since what is opened inside a closed
/// box does not show.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Opened {
    /// Parts, opened into their packages.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub parts: BTreeSet<String>,
    /// Packages, opened into what they hold.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub units: BTreeSet<String>,
    /// Steps of the route, by their path through it (`pkg.cli.build/2`).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub steps: BTreeSet<String>,
    /// Functions, every step that runs one opened at once.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub functions: BTreeSet<String>,
    /// Families of types, opened into their members.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub families: BTreeSet<String>,
    /// Places that stand for others, opened into them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub places: BTreeSet<String>,
    /// Directories, opened into their files.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dirs: BTreeSet<String>,
}

impl Opened {
    /// Nothing opened: the coarsest picture of the fibers kept.
    pub fn is_empty(&self) -> bool {
        [&self.parts, &self.units, &self.steps, &self.functions, &self.families, &self.places, &self.dirs].iter().all(|s| s.is_empty())
    }
}

/// A sight written as JSON, as the page's address and an agent write it.
impl FromStr for Sight {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Sight, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sight_reads_and_writes_as_the_page_does() {
        let text = r#"{"route":"fixture.cli.build","open":{"functions":["fixture.build.Pipeline.run"]}}"#;
        let sight: Sight = text.parse().unwrap();
        assert_eq!(sight.route.as_deref(), Some("fixture.cli.build"));
        assert_eq!(serde_json::to_string(&sight).unwrap(), text, "what is not given is left out");
        assert_eq!(serde_json::to_string(&Sight::default()).unwrap(), "{}");
        let kind: Sight = r#"{"kind":"cli","focus":["a","a"]}"#.parse().unwrap();
        assert_eq!((kind.kind, kind.focus.len()), (Some(EntryKind::Cli), 1), "a set holds a package once");
    }

    #[test]
    fn a_member_the_page_does_not_know_is_refused() {
        let e = "{\"rout\":\"x\"}".parse::<Sight>().unwrap_err().to_string();
        assert!(e.contains("unknown field `rout`"), "{e}");
        assert!("{\"open\":{\"step\":[\"x\"]}}".parse::<Sight>().is_err());
        assert!("{\"kind\":\"command\"}".parse::<Sight>().is_err(), "a kind is an entry's kind");
    }
}
