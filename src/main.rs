use chrono::prelude::*;
use chrono::{Duration, Local};
use colored::*;
use directories::BaseDirs;
use itertools::Itertools;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::{Display, Formatter, Write as FmtWrite};
use std::io::{BufRead, ErrorKind, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::{fmt, fs, io};

pub const CONFIG_FILENAME: &str = "ttrc.toml";
pub const SLOTS_PER_HOUR: usize = 4;
pub const DAY_SLOTS: usize = 24 * SLOTS_PER_HOUR;
pub const DAY_START: Slot = Slot(4 * SLOTS_PER_HOUR);
pub const DAY_CHART_STEP_SIZE: usize = 1;
pub const PRODUCTIVE_TARGET: f32 = 8.;
pub const COLORS: [&str; 7] = ["red", "green", "yellow", "blue", "magenta", "cyan", "white"];

fn get_input<T>() -> Option<T>
where
    T: FromStr,
{
    print!("?: ");
    io::stdout().flush().expect("flush");
    let stdin = io::stdin();
    let input: T = stdin.lock().lines().next()?.ok()?.parse().ok()?;
    Some(input)
}

fn get_base_dirs() -> BaseDirs {
    BaseDirs::new().expect("base_dirs")
}

type Shortcuts = Vec<Option<char>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Settings {
    editor: String,
    git: String,
    data_dir: PathBuf,
    git_repos_dir: PathBuf,
    activities: Vec<Activity>,
    #[serde(skip)]
    shortcuts: RefCell<Option<Shortcuts>>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            editor: "vim".to_string(),
            git: "/usr/bin/git".to_string(),
            git_repos_dir: PathBuf::from("/Users/hilbiga/git"),
            data_dir: get_base_dirs().data_dir().into(),
            activities: vec![],
            shortcuts: RefCell::new(None),
        }
    }
}

impl Settings {
    fn get_shortcut(&self, activity: &Activity) -> Option<char> {
        let index = self.activities.iter().position(|a| a == activity)?;
        self.get_shortcuts()[index]
    }

    fn get_shortcuts(&self) -> Shortcuts {
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

    fn get_filename_today(&self) -> String {
        let time = Local::now() - Duration::hours((*DAY_START / SLOTS_PER_HOUR) as i64);
        self.get_filename_by_date(
            time.year() as usize,
            time.month() as usize,
            time.day() as usize,
        )
    }

    fn get_filename_by_date(&self, year: usize, month: usize, day: usize) -> String {
        self.data_dir
            .join(format!("{}-{}-{}.json", year, month, day))
            .to_str()
            .unwrap()
            .into()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
struct Activity {
    name: String,
    productive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>
}

impl Activity {
    fn get_by_name(actis: &[Activity], name: &str) -> Option<Self> {
        actis.iter().find(|o| o.name == name).cloned()
    }

    fn prompt(settings: &Settings) -> Option<&Activity> {
        let shortcuts = settings.get_shortcuts();
        settings.activities.iter().enumerate().for_each(|(i, o)| {
            let mut name = o.to_string();
            if let Some(chr) = &shortcuts[i] {
                name = name.replacen(*chr, &format!("[{}]", chr), 1);
            }
            println!("\t{}: {}", i, name);
        });
        let input = get_input::<String>()?.trim().chars().next()?;
        let result = if input.is_numeric() {
            input.to_digit(10)
                .map(|number| number as usize)
                .filter(|number| *number < settings.activities.len())
                .map(|number| &settings.activities[number])
        } else if input.is_alphabetic() {
            shortcuts
                .iter()
                .position(|s| s.is_some() && s.unwrap() == input)
                .map(|i| &settings.activities[i])
        } else {
            None
        };
        if let Some(choice) = result {
            println!("~> {}", choice);
        }
        result
    }

    fn color(&self) -> &'static str {
        // maybe cache this...
        let color_idx = (self.name.chars().map(|c| c as usize).sum::<usize>() + self.name.len()) % COLORS.len();
        &COLORS[color_idx]
    }
}

impl Display for Activity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&*format!("{}", self.name.color(self.color())))?;
        if let Some(comment) = &self.comment {
            f.write_str(&*format!(" - {}", comment))?;
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Slot(usize);
impl Slot {
    #[cfg(not(test))]
    fn now() -> Slot {
        let local = Local::now();
        let hour = local.hour();
        let minute = local.minute();
        Slot::from_time(hour as usize, minute as usize)
    }

    /// Always return 12:00 for tests
    #[cfg(test)]
    fn now() -> Slot {
        Slot(16)
    }

    fn from_time(hour: usize, minute: usize) -> Slot {
        let minutes_per_slot = 60 / SLOTS_PER_HOUR;
        Slot(
            (((hour * SLOTS_PER_HOUR + minute / minutes_per_slot) as isize - *DAY_START as isize
                + DAY_SLOTS as isize) as usize)
                % DAY_SLOTS,
        )
    }

    fn next(&self) -> Slot {
        Slot(self.0 + 1)
    }
}

impl Deref for Slot {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Slot {
    type Error = Box<dyn std::error::Error>;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let hrs: usize;
        let min: usize;
        if text == "now" || text == "n" || text.is_empty() {
            return Ok(Slot::now());
        }
        if let Some((text_hrs, text_min)) = text.split_once(":") {
            hrs = text_hrs.parse()?;
            min = text_min.parse().unwrap_or(0);
        } else {
            hrs = text.parse()?;
            min = 0;
        }
        if hrs > 23 || min > 59 {
            Err(Box::new(io::Error::new(
                ErrorKind::InvalidInput,
                "out of range",
            )))
        } else {
            Ok(Slot::from_time(hrs, min))
        }
    }
}

impl Display for Slot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let shifted = (self.deref() + *DAY_START) % DAY_SLOTS;
        let hour = shifted / SLOTS_PER_HOUR;
        let minutes = (shifted % SLOTS_PER_HOUR) * (60 / SLOTS_PER_HOUR);
        f.write_str(&*format!("{:02}:{:02}", hour, minutes))?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Day {
    time_slots: Vec<Option<Activity>>,
}

impl Default for Day {
    fn default() -> Self {
        Day {
            time_slots: (0..DAY_SLOTS).into_iter().map(|_| None).collect(),
        }
    }
}

impl Day {
    pub fn entry_before_now(&self) -> Option<(Slot, &Activity)> {
        self.time_slots
            .iter()
            .take(*Slot::now())
            .enumerate()
            .rev()
            .find(|(_s, o)| o.is_some())
            .map(|(s, o)| (Slot(s), o.as_ref().unwrap()))
    }

    pub fn entry_before_now_mut(&mut self) -> Option<(Slot, &mut Activity)> {
        self.time_slots
            .iter_mut()
            .take(*Slot::now())
            .enumerate()
            .rev()
            .find(|(_s, o)| o.is_some())
            .map(|(s, o)| (Slot(s), o.as_mut().unwrap()))
    }

    pub fn slots(&self) -> impl Iterator<Item = (Slot, Slot, &Option<Activity>)> {
        self.time_slots
            .iter()
            .enumerate()
            .map(|(s, o)| (Slot(s), Slot(s).next(), o))
    }

    pub fn slots_collapsed<'a>(&'a self) -> impl Iterator<Item = (Slot, Slot, Option<Activity>)> + 'a {
        self.time_slots
            .iter()
            .cloned()
            .enumerate()
            .scan(None, |state: &mut Option<(usize, Option<Activity>)>, (i, o)| {
                if let Some((start, act)) = state {
                    if *act != o {
                        let result = Some(Some((Slot(*start), Slot(i), act.clone())));
                        *state = Some((i, o));
                        result
                    } else {
                        Some(None)
                    }
                } else {
                    *state = Some((i, o));
                    Some(None)
                }
            })
            .filter_map(|o| o)
    }

    pub fn first_non_empty(&self) -> Option<Slot> {
        self.time_slots.iter().position(|s| s.is_some()).map(Slot)
    }

    pub fn now_or_last_entry(&self) -> Slot {
        if let Some(entry) = self.entry_before_now() {
            entry.0.next()
        } else {
            Slot::now()
        }
    }

    pub fn hours_productive(&self) -> f32 {
        self.time_slots
            .iter()
            .filter_map(|it| it.as_ref())
            .filter(|it| it.productive)
            .count() as f32
            / SLOTS_PER_HOUR as f32
    }

    pub fn score(&self) -> f32 {
        self.hours_productive() as f32 / PRODUCTIVE_TARGET
    }

    pub fn activity_string(&self, settings: &Settings, step_by: usize) -> String {
        self.time_slots
            .iter()
            .step_by(step_by)
            .map(|s| {
                s.as_ref()
                    .and_then(|a| {
                        settings
                            .get_shortcut(&a)
                            .map(|s| s.to_string().color(a.color()).to_string())
                    })
                    .unwrap_or_else(|| " ".into())
            })
            .join("")
    }

    fn print_stats(&self, with_current_time: bool, trim_start: bool) {
        let first_non_empty = self.first_non_empty();
        self.slots_collapsed().for_each(|(s, e, o)| {
            if (!with_current_time || *s <= *Slot::now())
                && (!trim_start || first_non_empty.is_none() || *s >= *first_non_empty.unwrap())
            {
                println!(
                    "{}-{} - {}",
                    s,
                    e,
                    if let Some(act) = o {
                        act.to_string()
                    } else {
                        "empty".to_string()
                    }
                );
            }
        });
        println!(
            "Hours Productive: {}, Score: {:0.2} (time adjusted: {:0.2})",
            self.hours_productive(),
            self.score(),
            self.score() * (DAY_SLOTS as f32 / *Slot::now() as f32)
        );
    }

    fn write(&self, path: &Path) {
        fs::write(path, serde_json::to_string(&self).unwrap()).expect("write failed");
    }
}

struct UI<'d> {
    day: Day,
    file: PathBuf,
    settings: &'d Settings,
}

impl UI<'_> {
    fn print_current_slot_info(&self) {
        if let Some(entry) = self.day.entry_before_now() {
            
            println!("Recent activity: {} (until {})", entry.1, entry.0.next());
        }
        println!(
            "Current slot: {} ({})",
            Slot::now(),
            if let Some(act) = &self.day.time_slots[*Slot::now()] {
                format!("{}", act)
            } else {
                "no activity so far".bold().to_string()
            }
        );
    }

    fn get_git_commits(&self, start: Slot, end: Slot) -> Vec<String> {
        let today = Local::now();
        fs::read_dir(&self.settings.git_repos_dir).expect("read dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().expect("file type").is_dir())
            .map(|entry| entry.path())
            .map(|repo| {
                let repo_name = repo.file_name().unwrap().to_str().unwrap().clone();
                let log = Command::new(&self.settings.git)
                    .arg("log")
                    .arg("--oneline")
                    .arg("--after")
                    .arg(format!("{}-{:02}-{:02} {}", today.year(), today.month(), today.day(), start.to_string()))
                    .arg("--before")
                    .arg(format!("{}-{:02}-{:02} {}", today.year(), today.month(), today.day(), end.to_string()))
                    .current_dir(&repo)
                    .output()
                    .expect("failed to run git");
                String::from_utf8_lossy(&log.stdout)
                    .lines()
                    .map(|line| format!("{}: {}", repo_name, line))
                    .collect::<Vec<String>>()
            })
            .flatten()
            .collect::<Vec<String>>()
    }

    fn ask_about_activity(&mut self, start: Slot, end: Slot) {
        println!(
            "What did you do from {} - {}?",
            start.to_string().yellow(),
            end.to_string().yellow()
        );

        let act = Activity::prompt(&self.settings);
        if let Some(act) = act {
            for s in *start..*end {
                self.day.time_slots[s] = Some(act.clone());
            }

            let lines = self.get_git_commits(start, end);
            if !lines.is_empty() {
                println!("Include as comment: ");
                for (i, line) in lines.iter().enumerate() {
                    println!("{}: {}", i+1, line.color(Color::Yellow));
                }
                if let Some(index) = get_input::<usize>() {
                    if index > 0 && index-1 < lines.len() {
                        self.day.time_slots[*start].as_mut().unwrap().comment = Some(lines[index-1].clone());
                    } else {
                        println!("No comment included.");
                    }
                }
            }
        } else {
            println!("I didn't get that.");
        }
    }

    fn ask_about_activity_now(&mut self) {
        let now = *Slot::now();
        let start = *self.day.now_or_last_entry();
        let end = now + 1;
        self.ask_about_activity(Slot(start), Slot(end));
    }

    fn add_comment_to_last_activity(&mut self) {
        if let Some(entry) = self.day.entry_before_now_mut() {
            println!("Please enter a comment to add to {}.", entry.1);
            entry.1.comment = get_input();
        } else {
            println!("{}", "Please add a recent activity first!".red());
        }
    }

    fn edit_with_text_editor(&mut self) {
        let tmp_file = PathBuf::from_str("/tmp/time-track.tmp").unwrap();
        let mut data = String::new();
        writeln!(&mut data, "# Do not add or delete any lines in this document.");
        writeln!(&mut data, "# Edit the activities and associated comments by changing the text.");
        writeln!(&mut data, "# The time, activity name, and comment field (if any) must always be seperated by ' - '.");
        self.day.slots().for_each(|(s, e, o)| {
            let mut name = "empty";
            let mut comment = "".to_string();
            if let Some(act) = o {
                name = act.name.as_ref();
                if let Some(c) = act.comment.as_ref() {
                    comment = format!(" - {}", c.as_str());
                }
            }
            writeln!(
                &mut data,
                "{}-{} - {}{}",
                s,
                e,
                name,
                comment
            )
            .expect("write");
        });
        fs::write(&tmp_file, data).expect("write");
        let exit_code = Command::new(&self.settings.editor)
            .arg(tmp_file.to_str().unwrap())
            .status()
            .expect("could not open editor");
        if !exit_code.success() {
            println!("Editor exited with non-zero exit code!");
        } else {
            let data = fs::read_to_string(tmp_file).expect("could not read file");
            self.day.time_slots = data
                .lines()
                .filter(|o| !o.starts_with("#"))
                .map(|o| {
                    let mut splits = o.split(" - ");
                    splits.next().expect("format");
                    let mut activity = Activity::get_by_name(&self.settings.activities, splits.next().expect("format"));
                    if let Some(act) = activity.as_mut() {
                        act.comment = splits.next().map(|s| s.to_string());
                    }
                    activity
                })
                .collect();
        }
    }

    fn split(&mut self, only_one_split: bool) {
        let now_or_last_entry = self.day.now_or_last_entry();
        let possible_slots = (*now_or_last_entry + 1..*Slot::now() + 1)
            .into_iter()
            .collect::<Vec<_>>();
        if possible_slots.is_empty() {
            println!("{}", "There's nothing to split!".red());
            return;
        }
        let choice = if possible_slots.len() == 1 {
            Some(0)
        } else {
            println!("Where to split?");
            for (i, s) in possible_slots.iter().enumerate() {
                println!("{}: {}", i, Slot(*s));
            }
            get_input::<usize>()
        };
        if let Some(choice) = choice {
            self.ask_about_activity(now_or_last_entry, Slot(possible_slots[choice]));
            if !only_one_split {
                self.save();
                self.ask_about_activity(Slot(possible_slots[choice]), Slot::now().next());
            }
        }
    }

    /// Print statistics for multiple days. Might skip some days if the
    /// corresponding data files do not exist.
    fn multiday_statistics(&self, dates: impl Iterator<Item = DateTime<Local>>, print_days: bool) {
        let mut days = Vec::new();
        let step_by = DAY_CHART_STEP_SIZE;
        if print_days {
            println!(
                "{}{}",
                " ".repeat(36),
                (0..24)
                    .into_iter()
                    .step_by(step_by)
                    .map(|h| format!("{:<2}", (h + *DAY_START / SLOTS_PER_HOUR) % 24))
                    .join("  ")
            );
            println!("{}{}", " ".repeat(36), "| ".repeat(24 * (SLOTS_PER_HOUR / step_by / 2)))
        }
        for date in dates {
            let time = date.borrow();
            let file = PathBuf::from(self.settings.get_filename_by_date(
                time.year() as usize,
                time.month() as usize,
                time.day() as usize,
            ));
            if file.exists() {
                let day: Day = serde_json::from_str(
                    fs::read_to_string(file)
                        .expect("could not read file")
                        .as_str(),
                )
                .unwrap();
                if print_days {
                    println!(
                        "{}, {:02}.{:02}.: {:4.1} hrs., Score: {:0.2} {}",
                        time.weekday().to_string(),
                        time.day(),
                        time.month(),
                        day.hours_productive(),
                        day.score(),
                        day.activity_string(&self.settings, step_by)
                    );
                }
                days.push(day);
            } else if print_days {
                println!(
                    "{}, {:02}.{:02}.: no data",
                    time.weekday().to_string(),
                    time.day(),
                    time.month()
                );
            }
        }
        let hours: f32 = days.iter().map(|d| d.hours_productive()).sum();
        let hours_by_activity: HashMap<Activity, f32> = self
            .settings
            .activities
            .iter()
            .map(|activity| {
                (
                    activity.clone(),
                    days.iter()
                        .map(|d| {
                            d.time_slots
                                .iter()
                                .filter(|activity_at_time| {
                                    activity_at_time.is_some()
                                        && activity_at_time.as_ref().unwrap() == activity
                                })
                                .count() as f32
                                / SLOTS_PER_HOUR as f32
                        })
                        .sum(),
                )
            })
            .collect();
        let score = hours as f32 / (days.len() as f32 * PRODUCTIVE_TARGET);

        println!("Aggregated statistics from the last {} days:", days.len());
        println!("Hours Productive: {}, Score: {:0.2}", hours, score);
        println!(
            "Target: {} x {} = {} hours; Difference: {:+} hours",
            PRODUCTIVE_TARGET,
            days.len(),
            PRODUCTIVE_TARGET * days.len() as f32,
            hours - (PRODUCTIVE_TARGET * days.len() as f32)
        );
        hours_by_activity
            .iter()
            .sorted_unstable_by_key(|(_, hours)| (**hours * -2.) as isize)
            .enumerate()
            .for_each(|(i, (activity, hours))| {
                let str = format!("{:4.1} hrs. {}", hours, activity);
                if i % 2 == 1 || i == hours_by_activity.len() - 1 {
                    println!("{}", str);
                } else {
                    print!("{:40}", str);
                }
            });
    }

    fn save(&self) {
        self.day.write(&self.file);
    }
}

fn main() {
    let settings_file = get_base_dirs()
        .config_dir()
        .join(CONFIG_FILENAME.to_string());
    if !settings_file.exists() {
        let mut settings = Settings::default();
        settings.activities.push(Activity {
            name: "Example".to_string(),
            productive: false,
            comment: None,
        });
        settings.activities.push(Activity {
            name: "Second Example".to_string(),
            productive: true,
            comment: None,
        });
        let settings_str = toml::to_string(&settings).expect("seriaize");
        fs::write(&settings_file, settings_str).expect("write");
        println!("I have created a new config file here: {:?}", settings_file);
        println!("Please edit it and restart the program! :)");
        return;
    }
    let settings: Settings = toml::from_str(
        fs::read_to_string(&settings_file)
            .expect("read settings")
            .as_str(),
    )
    .expect("parse settingsa");

    let file = PathBuf::from(settings.get_filename_today());
    let day = if file.exists() {
        serde_json::from_str(
            fs::read_to_string(file.clone())
                .expect("could not read file")
                .as_str(),
        )
        .unwrap()
    } else {
        println!("Using new file {:?}", file);
        Day::default()
    };
    assert_eq!(day.time_slots.len(), DAY_SLOTS, "Loaded day file {} is invalid.", file.display());
    let mut ui = UI {
        day,
        file: file.clone(),
        settings: &settings,
    };
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "h" | "help" => {
                println!("Commands:");
                println!("\tactivity (a): Enter an activity for a specific time span.");
                println!("\tcomment (c): Add comment to last activity.");
                println!("\tday (d): Print statistics for a specific day.");
                println!("\tyesterday (yd): Print statistics for yesterday.");
                println!("\tedit (e): Edit activities for today in text editor.");
                println!("\tpath (p): Print today's data file path.");
                println!("\tsplit (s): Split the time since the last recorded activity in two.");
                println!("\ttoday (t): Print statistics for today.");
                println!("\tuntil (u): Like split, but only enter the first activity.");
                println!("\tweek (w, 2w, 3w): Print statistics for last seven, 14, 21 days.");
                println!("\tyear (y): Print statistics for last year.");
                println!();
                println!("Current data file: {:?}", &file);
                println!("Config file: {:?}", &settings_file);
            },
            "p" | "path" => {
                println!("{}", file.display());
            }
            "a" | "activity" => {
                ui.print_current_slot_info();
                println!(
                    "(Enter '{}' or a time like '{}' or just '{}'. Leave {} for 'now'.)",
                    "now".bright_blue(),
                    "18:10".bright_blue(),
                    "18".bright_blue(),
                    "empty".bright_blue()
                );
                println!("Start time:");
                let start = get_input::<String>().and_then(|s| Slot::try_from(s).ok());
                if let Some(start) = start {
                    println!("~> {}", start.to_string().bold());
                    println!("End time:");
                    let end = get_input::<String>().and_then(|s| Slot::try_from(s).ok());
                    if let Some(end) = end {
                        println!("~> {}", end.to_string().bold());
                        if *end <= *start {
                            println!("{}", "End time <= start time!".red());
                        } else {
                            ui.ask_about_activity(start, end);
                        }
                    } else {
                        println!("Invalid input.");
                    }
                } else {
                    println!("Invalid input.");
                }
            },
            "d" | "day" => {
                let time = Local::now() - Duration::hours((*DAY_START / SLOTS_PER_HOUR) as i64);
                let default_year = time.year() as usize;
                let default_month = time.month() as usize;
                let default_day = time.day() as usize;
                print!("Year [{}] ", default_year);
                let year = get_input::<usize>().unwrap_or(default_year);
                print!("Month [{}] ", default_month);
                let month = get_input::<usize>().unwrap_or(default_month);
                print!("Day [{}] ", default_day);
                let day = get_input::<usize>().unwrap_or(default_day);
                let file = PathBuf::from(settings.get_filename_by_date(year, month, day));
                println!("Loading file {:?}", file);
                let day: Day = serde_json::from_str(
                    fs::read_to_string(file)
                        .expect("could not read file")
                        .as_str(),
                )
                .unwrap();
                day.print_stats(false, true);
            },
            "yd" | "yesterday" => {
                let time = Local::now() - Duration::hours((*DAY_START / SLOTS_PER_HOUR) as i64) - Duration::days(1);
                let file = PathBuf::from(settings.get_filename_by_date(time.year() as usize, time.month() as usize, time.day() as usize));
                println!("Loading file {:?}", file);
                let day: Day = serde_json::from_str(
                    fs::read_to_string(file)
                        .expect("could not read file")
                        .as_str(),
                )
                    .unwrap();
                day.print_stats(false, true);
            },
            "t" | "today" => {
                ui.print_current_slot_info();
                ui.day.print_stats(true, true);
            },
            "w" | "week" => {
                ui.multiday_statistics(
                    (0..7).rev().map(|i| Local::now() - Duration::days(i)),
                    true,
                );
            },
            "2w" | "2week" => {
                ui.multiday_statistics(
                    (0..14).rev().map(|i| Local::now() - Duration::days(i)),
                    true,
                );
            },
            "3w" | "3week" => {
                ui.multiday_statistics(
                    (0..21).rev().map(|i| Local::now() - Duration::days(i)),
                    true,
                );
            },
            "y" | "year" => {
                ui.multiday_statistics(
                    (0..365).rev().map(|i| Local::now() - Duration::days(i)),
                    false,
                );
            },
            "e" | "edit" => {
                ui.print_current_slot_info();
                ui.edit_with_text_editor();
            },
            "s" | "split" => {
                ui.print_current_slot_info();
                ui.split(false);
            },
            "u" | "until" => {
                ui.print_current_slot_info();
                ui.split(true);
            },
            "c" | "comment" => {
                ui.print_current_slot_info();
                ui.add_comment_to_last_activity();
            },
            "json" => {
                let day_maps = (0..365).rev()
                    .map(|i| Local::now() - Duration::days(i))
                    .filter_map(|time| {
                        let file = PathBuf::from(settings.get_filename_by_date(
                            time.year() as usize,
                            time.month() as usize,
                            time.day() as usize,
                        ));
                        if file.exists() {
                            Some(serde_json::from_str(
                                fs::read_to_string(file)
                                    .expect("read file")
                                    .as_str()
                            ).expect("deserialize"))
                        } else {
                            None
                        }
                    })
                    .map(|d: Day| {
                        d.time_slots.iter()
                            .fold(HashMap::default(), |mut map: HashMap<Activity, usize>, slot| {
                                if let Some(activity) = slot {
                                    *map.entry(activity.clone())
                                        .or_insert(0) += 1;
                                }
                                map
                            })
                    })
                    .collect_vec();
                println!("{{");
                for activity in &settings.activities {
                    print!("\t\"{}\": [\n\t\t", activity.name);
                    for day in day_maps.iter() {
                        let &half_hours = day.get(activity).unwrap_or(&0);
                        print!("{}, ", half_hours as f32 / SLOTS_PER_HOUR as f32);
                    }
                    println!("\n\t],");
                }
                println!("}}");
            }
            arg => {
                println!("{}{}", "Unknown command: ".red(), arg);
                ui.print_current_slot_info();
                ui.ask_about_activity_now()
            },
        }
    } else {
        ui.print_current_slot_info();
        ui.ask_about_activity_now();
    }
    ui.save();
}

mod tests {
    #[cfg(test)]
    use super::*;

    #[test]
    pub fn test_first_non_empty() {
        let activity = Activity {
            name: "a".to_string(),
            productive: false,
        };
        let mut day = Day::default();
        day.time_slots[4 * 2] = Some(activity);
        assert!(day.first_non_empty().is_some());
        assert_eq!(day.first_non_empty().unwrap(), Slot(4 * 2));
        assert_eq!(day.now_or_last_entry(), Slot(4 * 2 + 1));

        let day = Day::default();
        assert!(day.first_non_empty().is_none());
        assert_eq!(day.now_or_last_entry(), Slot::now());
    }

    #[test]
    pub fn test_slots() {
        let slot = Slot(0);
        assert_eq!(*slot, 0);
        assert_eq!(format!("{}", slot), "04:00");
        let slot = Slot(47);
        assert_eq!(format!("{}", slot), "03:30");
        let slot = Slot::now();
        assert_eq!(format!("{}", slot), "12:00");
    }

    #[test]
    pub fn test_slot_from_string() {
        let slots = vec![
            Slot::try_from("18:00".to_string()),
            Slot::try_from("18:".to_string()),
            Slot::try_from("18".to_string()),
            Slot::try_from("18:3".to_string()),
            Slot::try_from("18:03".to_string()),
            Slot::try_from("18:30".to_string()),
            Slot::try_from("18:59".to_string()),
            Slot::try_from(":".to_string()),
            Slot::try_from("".to_string()),
            Slot::try_from("500:".to_string()),
            Slot::try_from(":30".to_string()),
            Slot::try_from("now".to_string()),
            Slot::try_from("n".to_string()),
            Slot::try_from("n:".to_string()),
        ];
        assert!(slots[0].is_ok());
        assert_eq!(*slots[0].as_ref().unwrap(), Slot(18 * 2 - *DAY_START));
        assert!(slots[1].is_ok());
        assert_eq!(*slots[1].as_ref().unwrap(), Slot(18 * 2 - *DAY_START));
        assert!(slots[2].is_ok());
        assert_eq!(*slots[2].as_ref().unwrap(), Slot(18 * 2 - *DAY_START));
        assert!(slots[3].is_ok());
        assert_eq!(*slots[3].as_ref().unwrap(), Slot(18 * 2 - *DAY_START));
        assert!(slots[4].is_ok());
        assert_eq!(*slots[4].as_ref().unwrap(), Slot(18 * 2 - *DAY_START));
        assert!(slots[5].is_ok());
        assert_eq!(*slots[5].as_ref().unwrap(), Slot(18 * 2 + 1 - *DAY_START));
        assert!(slots[6].is_ok());
        assert_eq!(*slots[6].as_ref().unwrap(), Slot(18 * 2 + 1 - *DAY_START));
        assert!(slots[7].is_err());
        assert!(slots[8].is_ok());
        assert_eq!(*slots[8].as_ref().unwrap(), Slot::now());
        assert!(slots[9].is_err());
        assert!(slots[10].is_err());
        assert!(slots[11].is_ok());
        assert_eq!(*slots[11].as_ref().unwrap(), Slot::now());
        assert!(slots[12].is_ok());
        assert_eq!(*slots[12].as_ref().unwrap(), Slot::now());
        assert!(slots[11].is_ok());
        assert!(slots[13].is_err());
    }

    #[test]
    pub fn test_get_by_name() {
        let activities = vec![
            Activity {
                name: "a".to_string(),
                productive: false,
            },
            Activity {
                name: "b".to_string(),
                productive: false,
            },
        ];
        assert_eq!(
            Activity::get_by_name(&activities, &*activities[0].name),
            Some(activities[0].clone())
        );
        assert_eq!(Activity::get_by_name(&activities, "empty"), None);
    }
}
