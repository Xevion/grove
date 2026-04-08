use std::path::PathBuf;

#[derive(Clone)]
pub struct Bookmark {
    pub label: &'static str,
    pub path: PathBuf,
    pub exists: bool,
}

pub fn default_bookmarks() -> Vec<Bookmark> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    [
        ("Home", home.clone()),
        ("Desktop", home.join("Desktop")),
        ("Documents", home.join("Documents")),
        ("Downloads", home.join("Downloads")),
        ("Projects", home.join("projects")),
        ("/", PathBuf::from("/")),
    ]
    .into_iter()
    .map(|(label, path)| {
        let exists = path.exists();
        Bookmark {
            label,
            path,
            exists,
        }
    })
    .collect()
}
