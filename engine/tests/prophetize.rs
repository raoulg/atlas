use std::collections::BTreeMap;
use std::path::PathBuf;

use atlas_engine::decide::prophetize;
use atlas_engine::{analyze, Options};

/// A vision decided in a file of any extension its language reads is marked in that language's
/// form: a JavaScript module and an ES module of TypeScript take the JSDoc tag a `.ts` file takes,
/// and Atlas reads each back as the prophecy it wrote.
#[test]
fn a_vision_is_marked_in_every_file_its_language_reads() {
    for ext in ["js", "mts"] {
        let root = std::env::temp_dir().join(format!("atlas-prophetize-{ext}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("makes the project");
        std::fs::write(root.join("package.json"), "{\"name\": \"shop\", \"version\": \"0.0.0\", \"type\": \"module\"}\n").expect("writes");
        let file = format!("src/index.{ext}");
        let now = "/** What a shop sells. */\nexport function stock() {\n  return [];\n}\n";
        std::fs::write(root.join(&file), now).expect("writes the module");
        let vision =
            BTreeMap::from([(PathBuf::from(&file), Some(format!("{now}\n/** Where orders wait. */\nexport function queue() {{}}\n")))]);
        let decided = prophetize(&root, &vision, &BTreeMap::new(), &Options::default()).expect("a vision that only adds is decided");
        assert!(decided.notes.is_empty(), "{ext}: {:?}", decided.notes);
        let text = &decided.files[&PathBuf::from(&file)];
        assert!(text.contains(" * Where orders wait.\n * @prophecy\n */\nexport function queue"), "{ext}: {text}");
        std::fs::write(root.join(&file), text).expect("writes the decision");
        let g = analyze(&root, &Options::default()).expect("reads");
        let marked: Vec<&str> = g.nodes.iter().filter(|n| n.prophecy().is_some()).map(|n| n.name.as_str()).collect();
        assert_eq!(marked, ["queue"], "{ext}");
        assert_eq!(decided.marked.len(), 1, "{ext}: {:?}", decided.marked);
        let _ = std::fs::remove_dir_all(&root);
    }
}
