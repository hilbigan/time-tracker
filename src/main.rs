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

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Occupation {
    name: String,
    productive: bool
}

impl Occupation {
    fn get_all() -> Vec<Occupation> {
        vec![
            Occupation { name: "Arbeit".to_string(), productive: true },
            Occupation { name: "Baustelle".to_string(), productive: true },
            Occupation { name: "Hobby".to_string(), productive: true },
            Occupation { name: "Pause".to_string(), productive: false },
            Occupation { name: "Programmieren".to_string(), productive: true },
            Occupation { name: "Uni".to_string(), productive: true },
            Occupation { name: "Unterwegs".to_string(), productive: false },
            Occupation { name: "Zocken".to_string(), productive: false },
        ]
    }
    
    fn prompt(occps: &[Occupation]) -> Option<&Occupation> {
        occps.iter().enumerate().for_each(|(i, o)| {
            println!("\t{}: {}", i, o);
        });
        let number = get_input()?;
        Some(&occps[number])
    }
}

impl Display for Occupation {
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
    time_slots: Vec<Option<Occupation>>
}

impl Default for Day {
    fn default() -> Self {
        Day { time_slots: (0..DAY_SLOTS).into_iter().map(|_| None).collect() }
    }
}

impl Day {
    pub fn entry_before_now(&self) -> Option<(SlotRef, &Occupation)> {
        (*DAY_START .. *SlotRef::now()).rev().into_iter()
            .map(|s| (s, &self.time_slots[s])).find(|it| it.1.is_some())
            .and_then(|(s, o)| Some((SlotRef(s), o.as_ref().unwrap())))
    }
    
    pub fn slots(&self) -> impl Iterator<Item = (SlotRef, &Option<Occupation>)> {
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
}

fn get_filename() -> String {
    let time = Local::now() - Duration::hours((*DAY_START / 2) as i64);
    get_filename_by_date(time.year() as usize, time.month() as usize, time.day() as usize)
}

fn get_filename_by_date(year: usize, month: usize, day: usize) -> String {
    format!("/home/aaron/.local/share/{}-{}-{}.json", year, month, day)
}

fn main() {
    let occupations = Occupation::get_all();
    let file = PathBuf::from(get_filename());
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
             if let Some(occ) = &day.time_slots[now] {
                 format!("{}", occ)
             } else {
                 "no activity so far".bold().to_string()
             }
    );
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "h" | "help" => {
                println!("Commands:");
                println!("\ttoday (t): Print statistics for today.");
                println!("\tday (d): Print statistics for certain day.");
                println!("\tsplit (s): Split the time since the last recorded activity in two.");
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
                let file = serde_json::from_str(fs::read_to_string(file).expect("could not read file").as_str()).unwrap();
                print_day_stats(&file);
            },
            "t" | "today" => {
                print_day_stats(&day);
            },
            "s" | "split" => {
                let now_or_last_entry = day.now_or_last_entry();
                let possible_slots = (*now_or_last_entry + 1 .. *SlotRef::now() + 1).into_iter().collect::<Vec<_>>();
                if possible_slots.is_empty() {
                    println!("{}", "There's nothing to split!".red());
                    return;
                }
                println!("Where to split?");
                for (i, s) in possible_slots.iter().enumerate() {
                    println!("{}: {}", i, SlotRef(*s));
                }
                if let Some(choice) = get_input() {
                    ask_about_activity(&mut day, &occupations, now_or_last_entry, SlotRef(possible_slots[choice]));
                    save_file(&mut day, file.as_path());
                    ask_about_activity(&mut day, &occupations, SlotRef(possible_slots[choice]), SlotRef::now().next());
                }
            },
            _ => ask_about_activity_now(&mut day, &occupations)
        }
    } else {
        ask_about_activity_now(&mut day, &occupations);
    }
    save_file(&mut day, file.as_path());
}

fn print_day_stats(day: &Day) {
    let first_non_empty = day.first_non_empty();
    day.slots().for_each(|(s, o)| {
        if *s <= *SlotRef::now() && (first_non_empty.is_none() || *s >= *first_non_empty.unwrap()) {
            println!("{}-{} - {}", s, s.next(), if let Some(occ) = o { occ.to_string() } else { "empty".to_string() });
        }
    });
    println!("Hours Productive: {}, Score: {:0.2}", day.hours_productive(), day.score());
}

fn save_file(day: &mut Day, path: &Path) {
    fs::write(path, serde_json::to_string(&day).unwrap()).expect("write failed");
}

fn ask_about_activity(day: &mut Day, occs: &Vec<Occupation>, start: SlotRef, end: SlotRef) {
    println!("What did you do from {} - {}?", start.to_string().yellow(), end.to_string().yellow());
    let occ = Occupation::prompt(&occs);
    if let Some(occ) = occ {
        for s in *start .. *end {
            day.time_slots[s] = Some(occ.clone());
        }
    }
}

fn ask_about_activity_now(day: &mut Day, occs: &Vec<Occupation>) {
    let now = *SlotRef::now();
    let start = day.now_or_last_entry();
    let end = now + 1;
    ask_about_activity(day, occs, start, SlotRef(end));
}

#[test]
pub fn test_slots() {
    let slot = SlotRef(0);
    assert_eq!(*slot, 0);
    assert_eq!(format!("{}", slot), "04:00");
    let slot = SlotRef(47);
    assert_eq!(format!("{}", slot), "03:30");
}
