use chrono::{Local, Duration};
use chrono::prelude::*;
use std::fmt::{Display, Formatter};
use std::error::Error;
use std::{fmt, io};
use std::ops::{Deref, Sub};
use std::path::{PathBuf, Path};
use std::fs;
use serde_derive::{Serialize, Deserialize};
use std::str::FromStr;
use std::io::{BufRead, Write};
use colored::*;
use std::process::Command;
use std::fmt::Write as FmtWrite;

pub const EDITOR: &str = "nvim";
pub const DAY_SLOTS: usize = 48;
pub const DAY_START: SlotRef = SlotRef(8);
pub const COLORS: [&str; 7] = [
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
];

fn get_input() -> Option<usize> {
    print!("?: ");
    io::stdout().flush();
    let stdin = io::stdin();
    let number: usize = stdin.lock().lines().next()?.ok()?.parse().ok()?;
    Some(number)
}

fn get_filename_today() -> String {
    let time = Local::now() - Duration::hours((*DAY_START / 2) as i64);
    get_filename_by_date(time.year() as usize, time.month() as usize, time.day() as usize)
}

fn get_filename_by_date(year: usize, month: usize, day: usize) -> String {
    format!("/home/aaron/.local/share/{}-{}-{}.json", year, month, day)
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
struct Activity {
    name: String,
    productive: bool
}

impl Activity {
    fn get_all() -> Vec<Activity> {
        vec![
            Activity { name: "Arbeit".to_string(), productive: true },
            Activity { name: "Baustelle".to_string(), productive: true },
            Activity { name: "Hobby".to_string(), productive: true },
            Activity { name: "Pause".to_string(), productive: false },
            Activity { name: "Programmieren".to_string(), productive: true },
            Activity { name: "Uni".to_string(), productive: true },
            Activity { name: "Unterwegs".to_string(), productive: false },
            Activity { name: "Zocken".to_string(), productive: false },
        ]
    }
    
    fn get_by_name(actis: &[Activity], name: &str) -> Option<Self> {
        actis.iter().find(|o| o.name == name).cloned()
    }
    
    fn prompt(actis: &[Activity]) -> Option<&Activity> {
        actis.iter().enumerate().for_each(|(i, o)| {
            println!("\t{}: {}", i, o);
        });
        let number = get_input()?;
        Some(&actis[number])
    }
}

impl Display for Activity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let color_idx = self.name.chars().map(|c| c as usize).sum::<usize>() % COLORS.len();
        f.write_str(&*format!("{}", self.name.color(COLORS[color_idx])))?;
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub struct SlotRef(usize);
impl SlotRef {
    fn now() -> SlotRef {
        let local = Local::now();
        let hour = local.hour();
        let minute = local.minute();
        SlotRef((((hour * 2 + if minute > 30 { 1 } else { 0 }) as isize - *DAY_START as isize + DAY_SLOTS as isize) as usize) % DAY_SLOTS)
    }
    
    fn next(&self) -> SlotRef {
        SlotRef(self.0 + 1)
    }
    
    fn previous(&self) -> SlotRef {
        SlotRef(self.0 - 1)
    }
}

impl Deref for SlotRef {
    type Target = usize;
    
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for SlotRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let shifted = (self.deref() + *DAY_START) % DAY_SLOTS;
        let hour = shifted / 2;
        let half = (shifted % 2) * 30;
        f.write_str(&*format!("{:02}:{:02}", hour, half))?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Day {
    time_slots: Vec<Option<Activity>>
}

impl Default for Day {
    fn default() -> Self {
        Day { time_slots: (0..DAY_SLOTS).into_iter().map(|_| None).collect() }
    }
}

impl Day {
    pub fn entry_before_now(&self) -> Option<(SlotRef, &Activity)> {
        (*DAY_START .. *SlotRef::now()).rev().into_iter()
            .map(|s| (s, &self.time_slots[s])).find(|it| it.1.is_some())
            .and_then(|(s, o)| Some((SlotRef(s), o.as_ref().unwrap())))
    }
    
    pub fn slots(&self) -> impl Iterator<Item = (SlotRef, &Option<Activity>)> {
        self.time_slots.iter().enumerate().map(|(s, o)| {
            (SlotRef(s), o)
        })
    }
    
    pub fn first_non_empty(&self) -> Option<SlotRef> {
        self.time_slots.iter().enumerate().find(|(s, o)| o.is_some()).map(|(s, _)| SlotRef(s))
    }
    
    pub fn now_or_last_entry(&self) -> SlotRef {
        if let Some(entry) = self.entry_before_now() {
            entry.0.next()
        } else {
            SlotRef::now()
        }
    }
    
    pub fn hours_productive(&self) -> f32 {
        self.time_slots.iter().filter_map(|it| it.as_ref()).filter(|it| it.productive).count() as f32 / 2.
    }
    
    pub fn score(&self) -> f32 {
        self.hours_productive() as f32 / 12.
    }
    
    fn print_stats(&self, with_current_time: bool, trim_start: bool) {
        let first_non_empty = self.first_non_empty();
        self.slots().for_each(|(s, o)| {
            if (!with_current_time || *s <= *SlotRef::now()) && (!trim_start || first_non_empty.is_none() || *s >= *first_non_empty.unwrap()) {
                println!("{}-{} - {}", s, s.next(), if let Some(act) = o { act.to_string() } else { "empty".to_string() });
            }
        });
        println!("Hours Productive: {}, Score: {:0.2}", self.hours_productive(), self.score());
    }
    
    fn write(&self, path: &Path) {
        fs::write(path, serde_json::to_string(&self).unwrap()).expect("write failed");
    }
}

struct UI {
    day: Day,
    file: PathBuf,
    activities: Vec<Activity>
}

impl UI {
    fn ask_about_activity(&mut self, start: SlotRef, end: SlotRef) {
        println!("What did you do from {} - {}?", start.to_string().yellow(), end.to_string().yellow());
        let act = Activity::prompt(&self.activities);
        if let Some(act) = act {
            for s in *start .. *end {
                self.day.time_slots[s] = Some(act.clone());
            }
        }
    }
    
    fn ask_about_activity_now(&mut self) {
        let now = *SlotRef::now();
        let start = self.day.now_or_last_entry();
        let end = now + 1;
        self.ask_about_activity(start, SlotRef(end));
    }
    
    fn edit_with_text_editor(&mut self) {
        let tmp_file = PathBuf::from_str("/tmp/time-track.tmp").unwrap();
        let mut data = String::new();
        self.day.slots().for_each(|(s, o)| {
            writeln!(&mut data, "{}-{} - {}", s, s.next(), if let Some(act) = o { act.name.clone() } else { "empty".to_string() });
        });
        fs::write(&tmp_file, data);
        let exit_code = Command::new(EDITOR)
            .arg(tmp_file.to_str().unwrap())
            .status()
            .expect("could not open editor");
        if !exit_code.success() {
            println!("Editor exited with non-zero exit code!");
        } else {
            let data = fs::read_to_string(tmp_file).expect("could not read file");
            self.day.time_slots = data.lines().enumerate().map(|(i, o)| {
                Activity::get_by_name(&self.activities, &o[14..])
            }).collect();
        }
    }
    
    fn split(&mut self) {
        let now_or_last_entry = self.day.now_or_last_entry();
        let possible_slots = (*now_or_last_entry + 1 .. *SlotRef::now() + 1).into_iter().collect::<Vec<_>>();
        if possible_slots.is_empty() {
            println!("{}", "There's nothing to split!".red());
            return;
        }
        let choice = if possible_slots.len() == 1 {
            Some(0)
        } else {
            println!("Where to split?");
            for (i, s) in possible_slots.iter().enumerate() {
                println!("{}: {}", i, SlotRef(*s));
            }
            get_input()
        };
        if let Some(choice) = choice {
            self.ask_about_activity(now_or_last_entry, SlotRef(possible_slots[choice]));
            self.day.write(self.file.as_path());
            self.ask_about_activity(SlotRef(possible_slots[choice]), SlotRef::now().next());
        }
    }
    
    fn save(&self) {
        self.day.write(&self.file);
    }
}

fn main() {
    let activities = Activity::get_all();
    let file = PathBuf::from(get_filename_today());
    let mut day = if file.exists() {
        println!("Reading from {:?}", file);
        serde_json::from_str(fs::read_to_string(file.clone()).expect("could not read file").as_str()).unwrap()
    } else {
        println!("Using new file {:?}", file);
        Day::default()
    };
    let now = *SlotRef::now();
    if let Some(entry) = day.entry_before_now() {
        println!("Recent activity: {} (until {})", entry.1, entry.0.next());
    }
    println!("Current slot: {} ({})", SlotRef::now(),
             if let Some(act) = &day.time_slots[now] {
                 format!("{}", act)
             } else {
                 "no activity so far".bold().to_string()
             }
    );
    let mut ui = UI { day, file, activities };
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "h" | "help" => {
                println!("Commands:");
                println!("\ttoday (t): Print statistics for today.");
                println!("\tday (d): Print statistics for certain day.");
                println!("\tweek (w): Print statistics for last seven days.");
                println!("\tsplit (s): Split the time since the last recorded activity in two.");
                println!("\tedit (e): Edit activities for today in text editor.");
            },
            "d" | "day" => {
                let time = Local::now() - Duration::hours((*DAY_START / 2) as i64);
                let default_year = time.year() as usize;
                let default_month = time.month() as usize;
                let default_day = time.day() as usize;
                print!("Year [{}] ", default_year);
                let year = get_input().unwrap_or(default_year);
                print!("Month [{}] ", default_month);
                let month = get_input().unwrap_or(default_month);
                print!("Day [{}] ", default_day);
                let day = get_input().unwrap_or(default_day);
                let file = PathBuf::from(get_filename_by_date(year, month, day));
                println!("Loading file {:?}", file);
                let day: Day = serde_json::from_str(fs::read_to_string(file).expect("could not read file").as_str()).unwrap();
                day.print_stats(false, true);
            },
            "t" | "today" => {
                ui.day.print_stats(true, true);
            },
            "w" | "week" => {
                let mut days = Vec::with_capacity(7);
                for i in (0..7).rev() {
                    let time = Local::now() - Duration::days(i);
                    let file = PathBuf::from(get_filename_by_date(time.year() as usize, time.month() as usize, time.day() as usize));
                    if file.exists() {
                        let day: Day = serde_json::from_str(fs::read_to_string(file).expect("could not read file").as_str()).unwrap();
                        println!("{}: {} hrs., Score: {:0.2}", time.weekday().to_string(), day.hours_productive(), day.score());
                        days.push(day);
                    }
                }
                println!("Aggregated statistics from the last {} days:", days.len());
                let hours: usize = days.iter().map(|d| d.hours_productive() as usize).sum();
                let score = hours as f32 / (days.len() as f32 * 12.);
                println!("Hours Productive: {}, Score: {:0.2}", hours, score);
            },
            "e" | "edit" => {
                ui.edit_with_text_editor();
            },
            "s" | "split" => {
                ui.split();
            },
            _ => ui.ask_about_activity_now()
        }
    } else {
        ui.ask_about_activity_now();
    }
    ui.save();
}

mod tests {
    use super::*;
    
    #[test]
    pub fn test_slots() {
        let slot = SlotRef(0);
        assert_eq!(*slot, 0);
        assert_eq!(format!("{}", slot), "04:00");
        let slot = SlotRef(47);
        assert_eq!(format!("{}", slot), "03:30");
    }
    
    #[test]
    pub fn test_get_by_name() {
        let activities = Activity::get_all();
        assert_eq!(Activity::get_by_name(&activities, &*activities[0].name), Some(activities[0].clone()));
        assert_eq!(Activity::get_by_name(&activities, "empty"), None);
    }
}
