use std::path::PathBuf;

use crate::icons::IconName;

#[derive(Clone)]
pub struct Bookmark {
    pub label: &'static str,
    pub path: PathBuf,
    pub icon: IconName,
    pub exists: bool,
}

pub fn default_bookmarks() -> Vec<Bookmark> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    [
        ("Home", home.clone(), IconName::Home),
        ("Desktop", home.join("Desktop"), IconName::Screen),
        ("Documents", home.join("Documents"), IconName::FileDoc),
        ("Downloads", home.join("Downloads"), IconName::Download),
        ("Projects", home.join("projects"), IconName::Code),
        ("/", PathBuf::from("/"), IconName::Server),
    ]
    .into_iter()
    .map(|(label, path, icon)| {
        let exists = path.exists();
        Bookmark {
            label,
            path,
            icon,
            exists,
        }
    })
    .collect()
}
