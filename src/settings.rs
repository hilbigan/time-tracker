use std::path::PathBuf;
use std::cell::RefCell;
use serde_derive::{Deserialize, Serialize};
use chrono::{Datelike, Duration, Local};
use crate::{DAY_START, SLOTS_PER_HOUR};
use crate::activity::Activity;

type Shortcuts = Vec<Option<char>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub editor: String,
    pub git: String,
    pub data_dir: PathBuf,
    pub git_repos_dir: PathBuf,
    pub git_author: String,
    pub activities: Vec<Activity>,
    #[serde(skip)]
    shortcuts: RefCell<Option<Shortcuts>>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            editor: "vim".to_string(),
            git: "/usr/bin/git".to_string(),
            git_repos_dir: PathBuf::from("/Users/hilbiga/git"),
            git_author: "Your Name".to_string(),
            data_dir: crate::get_base_dirs().data_dir().into(),
            activities: vec![],
            shortcuts: RefCell::new(None),
        }
    }
}

impl Settings {
    pub fn get_shortcut(&self, activity: &Activity) -> Option<char> {
        let index = self.activities.iter().position(|a| a == activity)?;
        self.get_shortcuts()[index]
    }

    pub fn get_shortcuts(&self) -> Shortcuts {
        if self.shortcuts.borrow().is_none() {
            let mut shortcuts = Vec::with_capacity(self.activities.len());
            for activity in &self.activities {
                if let Some(chr) = activity
                    .name
                    .chars()
                    .find(|c| !shortcuts.contains(&Some(*c)))
                {
                    shortcuts.push(Some(chr))
                } else {
                    shortcuts.push(None)
                }
            }
            *self.shortcuts.borrow_mut() = Some(shortcuts);
        }
        self.shortcuts.borrow().as_ref().unwrap().clone()
    }

    pub fn get_filename_today(&self) -> PathBuf {
        let time = Local::now() - Duration::hours((*DAY_START / SLOTS_PER_HOUR) as i64);
        self.get_filename_by_date(
            time.year() as usize,
            time.month() as usize,
            time.day() as usize,
        )
    }

    pub fn get_filename_by_date(&self, year: usize, month: usize, day: usize) -> PathBuf {
        self.data_dir
            .join(format!("{}-{}-{}.json", year, month, day))
    }
}
