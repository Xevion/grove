use std::path::PathBuf;

use crate::icons::IconName;

#[derive(Clone)]
pub struct Bookmark {
    pub label: &'static str,
    pub path: PathBuf,
    pub icon: IconName,
    pub exists: bool,
}

#[cfg(not(target_family = "wasm"))]
#[must_use]
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

#[cfg(target_family = "wasm")]
pub fn default_bookmarks() -> Vec<Bookmark> {
    vec![
        Bookmark {
            label: "Home",
            path: PathBuf::new(),
            icon: IconName::Home,
            exists: true,
        },
        Bookmark {
            label: "Desktop",
            path: PathBuf::from("Desktop"),
            icon: IconName::Screen,
            exists: true,
        },
        Bookmark {
            label: "Documents",
            path: PathBuf::from("Documents"),
            icon: IconName::FileDoc,
            exists: true,
        },
        Bookmark {
            label: "Downloads",
            path: PathBuf::from("Downloads"),
            icon: IconName::Download,
            exists: true,
        },
        Bookmark {
            label: "Projects",
            path: PathBuf::from("projects"),
            icon: IconName::Code,
            exists: true,
        },
    ]
}
