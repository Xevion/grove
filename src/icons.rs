use gpui::{Hsla, Rems, Styled, rems, svg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IconName {
    Archive,
    ArrowUp,
    ChevronRight,
    Code,
    Download,
    Eye,
    EyeOff,
    File,
    FileCode,
    FileDoc,
    FileGeneric,
    FileGit,
    FileMarkdown,
    FileRust,
    FileToml,
    Folder,
    FolderOpen,
    Home,
    Image,
    Json,
    Link,
    Plus,
    Refresh,
    Screen,
    Server,
    Terminal,
    Warning,
}

impl IconName {
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Archive => "icons/archive.svg",
            Self::ArrowUp => "icons/arrow_up.svg",
            Self::ChevronRight => "icons/chevron_right.svg",
            Self::Code => "icons/code.svg",
            Self::Download => "icons/download.svg",
            Self::Eye => "icons/eye.svg",
            Self::EyeOff => "icons/eye_off.svg",
            Self::File => "icons/file.svg",
            Self::FileCode => "icons/file_code.svg",
            Self::FileDoc => "icons/file_doc.svg",
            Self::FileGeneric => "icons/file_generic.svg",
            Self::FileGit => "icons/file_git.svg",
            Self::FileMarkdown => "icons/file_markdown.svg",
            Self::FileRust => "icons/file_rust.svg",
            Self::FileToml => "icons/file_toml.svg",
            Self::Folder => "icons/folder.svg",
            Self::FolderOpen => "icons/folder_open.svg",
            Self::Home => "icons/home.svg",
            Self::Image => "icons/image.svg",
            Self::Json => "icons/json.svg",
            Self::Link => "icons/link.svg",
            Self::Plus => "icons/plus.svg",
            Self::Refresh => "icons/refresh.svg",
            Self::Screen => "icons/screen.svg",
            Self::Server => "icons/server.svg",
            Self::Terminal => "icons/terminal.svg",
            Self::Warning => "icons/warning.svg",
        }
    }

    /// Returns an icon appropriate for a file based on its filename.
    #[must_use]
    pub fn for_filename(filename: &str) -> Self {
        // Check full filename first for dotfiles and special names
        match filename {
            ".gitignore" | ".gitmodules" | ".gitattributes" | ".gitconfig" => return Self::FileGit,
            "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml"
            | "Gemfile.lock" | "poetry.lock" | "flake.lock" => return Self::FileGeneric,
            _ => {}
        }

        // Fall back to extension-based lookup
        let ext = filename.rsplit('.').next().unwrap_or("");
        if ext == filename {
            // No extension (or the filename is the extension, e.g. "Makefile")
            return Self::File;
        }
        match ext {
            "rs" => Self::FileRust,
            "toml" => Self::FileToml,
            "md" | "mdx" => Self::FileMarkdown,
            "json" | "jsonc" | "json5" => Self::Json,
            "doc" | "docx" | "pdf" | "odt" | "rtf" | "txt" => Self::FileDoc,
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" | "avif" => {
                Self::Image
            }
            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" => Self::Archive,
            "sh" | "bash" | "zsh" | "fish" => Self::Terminal,
            "js" | "jsx" | "ts" | "tsx" | "py" | "rb" | "go" | "java" | "kt" | "c" | "cpp"
            | "h" | "hpp" | "cs" | "swift" | "zig" | "lua" | "ex" | "exs" | "hs" | "ml"
            | "css" | "scss" | "sass" | "less" | "html" | "htm" | "xml" | "yaml" | "yml" => {
                Self::FileCode
            }
            "lnk" | "symlink" => Self::Link,
            _ => Self::File,
        }
    }
}

pub struct Icon {
    name: IconName,
    size: Rems,
    color: Option<Hsla>,
}

#[allow(dead_code)]
impl Icon {
    #[must_use]
    pub const fn new(name: IconName) -> Self {
        Self {
            name,
            size: rems(1.0),
            color: None,
        }
    }

    #[must_use]
    pub const fn size(mut self, size: Rems) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub const fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl gpui::IntoElement for Icon {
    type Element = <gpui::Svg as gpui::IntoElement>::Element;

    fn into_element(self) -> Self::Element {
        let mut el = svg()
            .path(self.name.path())
            .size(self.size)
            .flex_none();
        if let Some(color) = self.color {
            el = el.text_color(color);
        }
        el.into_element()
    }
}
