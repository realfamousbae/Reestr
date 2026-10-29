//! Copies of the file next to it (D32): once a day when it is opened, the last ten kept.
//! A copy is the encrypted file as is, so it opens with the password it had that day.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use reestr_core::date::{format_iso, parse_iso};
use time::Date;

/// How many copies are kept.
pub const COPIES_KEPT: usize = 10;

/// The copies folder is named after the file: «Семья — копии».
const FOLDER_SUFFIX: &str = " — копии";
const EXTENSION: &str = ".reestr";

/// A copy of the file in its copies folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCopy {
    pub path: PathBuf,
    pub date: Date,
    pub size: u64,
}

/// What happened to the copy when the file was opened.
#[derive(Debug)]
pub enum CopyOutcome {
    Made(PathBuf),
    /// Today's copy is already there, or the file is a copy itself.
    Skipped,
    Failed(io::Error),
}

fn stem(file: &Path) -> Option<&str> {
    file.file_stem().and_then(|s| s.to_str())
}

/// The copies folder of `file`, or `None` when `file` lies in a copies folder itself: a copy
/// opened to look at it does not make copies of its own.
pub fn copies_dir(file: &Path) -> Option<PathBuf> {
    let parent = file.parent()?;
    let in_copies = parent
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(FOLDER_SUFFIX));
    if in_copies {
        return None;
    }
    Some(parent.join(format!("{}{FOLDER_SUFFIX}", stem(file)?)))
}

fn copy_name(stem: &str, date: Date) -> String {
    format!("{stem} {}{EXTENSION}", format_iso(date))
}

/// The copies of `file`, oldest first. Only files named like our copies are listed.
pub(crate) fn copies_of(file: &Path) -> io::Result<Vec<FileCopy>> {
    let (Some(dir), Some(stem)) = (copies_dir(file), stem(file)) else {
        return Ok(Vec::new());
    };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let prefix = format!("{stem} ");
    let mut copies = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let date = name
            .to_str()
            .and_then(|n| n.strip_prefix(&prefix))
            .and_then(|n| n.strip_suffix(EXTENSION))
            .and_then(parse_iso);
        if let Some(date) = date {
            copies.push(FileCopy {
                path: entry.path(),
                date,
                size: entry.metadata()?.len(),
            });
        }
    }
    copies.sort_by_key(|c| c.date);
    Ok(copies)
}

/// Copies `file` (open and checked, with no transaction running) unless today's copy exists,
/// then removes all but the last [`COPIES_KEPT`].
pub(crate) fn make_copy(file: &Path, today: Date) -> CopyOutcome {
    let (Some(dir), Some(stem)) = (copies_dir(file), stem(file)) else {
        return CopyOutcome::Skipped;
    };
    let name = copy_name(stem, today);
    let target = dir.join(&name);
    if target.exists() {
        return CopyOutcome::Skipped;
    }
    let result = (|| -> io::Result<()> {
        fs::create_dir_all(&dir)?;
        // A copy cut short must not look like a finished one.
        let partial = dir.join(format!(".{name}.part"));
        fs::copy(file, &partial)?;
        fs::rename(&partial, &target)?;
        let copies = copies_of(file)?;
        for old in &copies[..copies.len().saturating_sub(COPIES_KEPT)] {
            fs::remove_file(&old.path)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => CopyOutcome::Made(target),
        Err(e) => CopyOutcome::Failed(e),
    }
}
