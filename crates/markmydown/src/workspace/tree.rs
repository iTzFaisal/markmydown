//! Supported files and the sidebar tree model.
//!
//! The tree is deliberately separate from the UI. It is rebuilt from disk on
//! demand and contains only supported documents and directories. Hidden
//! entries, `.git`, `node_modules`, and `target` are skipped.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// Extensions the editor treats as editable Markdown documents.
pub const SUPPORTED_EXTENSIONS: [&str; 4] = ["md", "markdown", "mdown", "txt"];

/// The kind of document a supported extension describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Markdown,
    PlainText,
}

impl FileKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        match extension(path)?.as_str() {
            "md" | "markdown" | "mdown" => Some(Self::Markdown),
            "txt" => Some(Self::PlainText),
            _ => None,
        }
    }

    pub fn is_markdown(self) -> bool {
        matches!(self, Self::Markdown)
    }
}

/// Returns the lowercase extension of a path.
pub fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
}

/// Whether a path is one of the supported document types.
pub fn is_supported(path: &Path) -> bool {
    FileKind::from_path(path).is_some()
}

/// A file or directory in the workspace tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    fn new(path: PathBuf, name: String, is_dir: bool) -> Self {
        Self {
            path,
            name,
            is_dir,
            children: Vec::new(),
        }
    }

    pub fn kind(&self) -> Option<FileKind> {
        if self.is_dir {
            None
        } else {
            FileKind::from_path(&self.path)
        }
    }

    /// Depth-first list of supported files under this node.
    pub fn files<'a>(&'a self, out: &mut Vec<&'a TreeNode>) {
        if self.is_dir {
            for child in &self.children {
                child.files(out);
            }
        } else if !self.is_dir && is_supported(&self.path) {
            out.push(self);
        }
    }

    /// Depth-first search by exact path.
    pub fn find(&self, path: &Path) -> Option<&TreeNode> {
        if self.path == path {
            return Some(self);
        }

        self.children.iter().find_map(|child| child.find(path))
    }

    pub fn flatten<'a>(&'a self, out: &mut Vec<&'a TreeNode>) {
        out.push(self);
        for child in &self.children {
            child.flatten(out);
        }
    }
}

/// A scanned local folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTree {
    root: PathBuf,
    root_node: TreeNode,
}

impl WorkspaceTree {
    /// Scans `root` and builds the tree.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, TreeError> {
        let root = root.as_ref();
        let metadata = std::fs::metadata(root).map_err(TreeError::Io)?;

        if !metadata.is_dir() {
            return Err(TreeError::NotADirectory(root.to_path_buf()));
        }

        let root = std::fs::canonicalize(root).map_err(TreeError::Io)?;
        let name = root.file_name().map_or_else(
            || root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );

        let mut tree = Self {
            root: root.clone(),
            root_node: TreeNode::new(root, name, true),
        };
        tree.refresh()?;
        Ok(tree)
    }

    /// Rebuilds the tree from disk while keeping the same root.
    pub fn refresh(&mut self) -> Result<(), TreeError> {
        let mut builder = WalkBuilder::new(&self.root);
        builder
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .require_git(false)
            .follow_links(false)
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }

                let name = entry.file_name().to_string_lossy();
                !matches!(name.as_ref(), ".git" | "node_modules" | "target")
            });

        let mut root_node = TreeNode::new(self.root.clone(), self.root_name(), true);

        for entry in builder.build() {
            let entry = entry.map_err(|error| TreeError::Walk(error.to_string()))?;
            let path = entry.path();

            if path == self.root {
                continue;
            }

            let relative = path
                .strip_prefix(&self.root)
                .map_err(|_| TreeError::OutsideRoot(path.to_path_buf()))?;
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());

            if !is_dir && !is_supported(path) {
                continue;
            }

            insert(&mut root_node, &self.root, relative, is_dir);
        }

        sort_tree(&mut root_node);
        self.root_node = root_node;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_node(&self) -> &TreeNode {
        &self.root_node
    }

    /// All supported files in display order.
    pub fn files(&self) -> Vec<&TreeNode> {
        let mut files = Vec::new();
        self.root_node.files(&mut files);
        files
    }

    pub fn find(&self, path: &Path) -> Option<&TreeNode> {
        self.root_node.find(path)
    }

    /// Whether `path` is inside the workspace root.
    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    /// Safe relative path for display and logging.
    pub fn relative<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        path.strip_prefix(&self.root).ok()
    }

    fn root_name(&self) -> String {
        self.root.file_name().map_or_else(
            || self.root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

fn insert(root: &mut TreeNode, base: &Path, relative: &Path, is_dir: bool) {
    let components: Vec<_> = relative.components().collect();
    if components.is_empty() {
        return;
    }

    let mut node = root;
    for (index, component) in components.iter().enumerate() {
        let component = component.as_os_str().to_string_lossy().into_owned();
        let path = base.join(relative.iter().take(index + 1).collect::<PathBuf>());
        let last = index + 1 == components.len();

        let child_index = node
            .children
            .iter()
            .position(|child| child.name == component);

        let child_index = match child_index {
            Some(index) => index,
            None => {
                node.children.push(TreeNode::new(
                    path,
                    component,
                    if last { is_dir } else { true },
                ));
                node.children.len() - 1
            }
        };

        if last {
            node.children[child_index].is_dir = is_dir;
        }

        node = &mut node.children[child_index];
    }
}

fn sort_tree(node: &mut TreeNode) {
    node.children.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    for child in &mut node.children {
        sort_tree(child);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error("cannot read workspace: {0}")]
    Io(#[source] std::io::Error),
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("path is outside the workspace: {0}")]
    OutsideRoot(PathBuf),
    #[error("workspace scan failed: {0}")]
    Walk(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        std::fs::create_dir_all(root.join(".hidden")).expect("hidden dir");
        std::fs::create_dir_all(root.join("node_modules")).expect("node modules");

        for file in [
            "readme.md",
            "guide.markdown",
            "draft.mdown",
            "notes.txt",
            "notes/topic.md",
            "image.png",
            "page.html",
            ".hidden/secret.md",
            "node_modules/pkg/readme.md",
        ] {
            let path = root.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(path, "# text").expect("write fixture");
        }

        tmp
    }

    #[test]
    fn lists_supported_documents_and_hides_other_files() {
        let tmp = fixture();
        let tree = WorkspaceTree::open(tmp.path()).expect("open workspace");
        let names: Vec<_> = tree.files().iter().map(|node| node.name.as_str()).collect();

        assert_eq!(
            names,
            vec![
                "topic.md",
                "draft.mdown",
                "guide.markdown",
                "notes.txt",
                "readme.md"
            ]
        );
        assert!(tree.find(&tmp.path().join("image.png")).is_none());
        assert!(tree.find(&tmp.path().join("page.html")).is_none());
        assert!(tree.find(&tmp.path().join(".hidden/secret.md")).is_none());
        assert!(
            tree.find(&tmp.path().join("node_modules/pkg/readme.md"))
                .is_none()
        );
    }

    #[test]
    fn directories_sort_before_files() {
        let tmp = fixture();
        let tree = WorkspaceTree::open(tmp.path()).expect("open workspace");
        let children = &tree.root_node().children;

        assert!(children[0].is_dir);
        assert_eq!(children[0].name, "notes");
    }

    #[test]
    fn empty_folder_is_a_valid_workspace() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tree = WorkspaceTree::open(tmp.path()).expect("open workspace");

        assert!(tree.files().is_empty());
        assert!(tree.root_node().is_dir);
    }

    #[test]
    fn rejects_files_as_workspace_roots() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("file.md");
        std::fs::write(&file, "# hi").expect("write");

        assert!(matches!(
            WorkspaceTree::open(file),
            Err(TreeError::NotADirectory(_))
        ));
    }
}
