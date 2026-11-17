use std::fs::File;

/// Waits until what `path` holds is on the disk.
pub fn sync(path: &str) {
    File::open(path).unwrap().sync_all().unwrap();
}
