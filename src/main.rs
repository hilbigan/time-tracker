use chrono::prelude::*;
use chrono::{Duration, Local};
use colored::*;
use directories::BaseDirs;
use itertools::Itertools;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::{Display, Write as FmtWrite};
use std::io::{BufRead, Write};
use std::ops::Deref;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;
use std::{fs, io};
use activity::Activity;
use day::{Day, Slot};
use settings::Settings;

mod settings;
mod activity;
mod day;

pub const CONFIG_FILENAME: &str = "ttrc.toml";
pub const CONFIG_OVERRIDE_ENV_VAR: &str = "TT_CONFIG";
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
                    .arg("--author")
                    .arg(&self.settings.git_author)
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

            self.save();
        } else {
            println!("I didn't get that.");
        }
    }

    fn ask_about_start_and_end_time(&mut self) -> Option<(Slot, Slot)> {
        println!(
            "(Enter '{}' or a time like '{}' or just '{}'. Leave {} for 'now'.)",
            "now".bright_blue(),
            "18:10".bright_blue(),
            "18".bright_blue(),
            "empty".bright_blue()
        );
        println!("Start time:");
        let start = get_input::<String>().and_then(|s| Slot::try_from(s).ok());
        return if let Some(start) = start {
            println!("~> {}", start.to_string().bold());
            println!("End time:");
            let end = get_input::<String>().and_then(|s| Slot::try_from(s).ok());
            if let Some(end) = end {
                println!("~> {}", end.to_string().bold());
                if *end <= *start {
                    println!("{}", "End time <= start time!".red());
                    None
                } else {
                    Some((start, end))
                }
            } else {
                println!("Invalid input.");
                None
            }
        } else {
            println!("Invalid input.");
            None
        }
    }

    fn ask_about_activity_now(&mut self) {
        let now = *Slot::now();
        let start = *self.day.now_or_last_entry();
        let end = now + 1;
        self.ask_about_activity(Slot(start), Slot(end));
    }

    fn ask_about_day(&self) -> PathBuf {
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
        return self.settings.get_filename_by_date(year, month, day);
    }

    fn add_comment_to_last_activity(&mut self) {
        if let Some(entry) = self.day.time_slots.iter_mut().rev().filter_map(|o| o.as_mut()).next() {
            println!("Please enter a comment to add to {}.", entry);
            entry.comment = get_input();
            self.save();
        } else {
            println!("{}", "Please add a recent activity first!".red());
        }
    }

    fn edit_with_text_editor(&mut self) {
        let tmp_file = PathBuf::from_str("/tmp/time-track.tmp").unwrap();
        let mut data = String::new();
        writeln!(&mut data, "# Do not add or delete any lines in this document.").expect("write");
        writeln!(&mut data, "# Edit the activities and associated comments by changing the text.").expect("write");
        writeln!(&mut data, "# The time, activity name, and comment field (if any) must always be seperated by ' - '.").expect("write");
        self.day.slots().for_each(|(s, e, o)| {
            let name = o.as_ref().map(|a| a.name.as_ref()).unwrap_or("empty");
            let comment = o.as_ref()
                .and_then(|a| a.comment.as_ref())
                .map(|c| format!(" - {}", c.as_str()))
                .unwrap_or("".to_string());
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
            self.save();
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
            Some(Slot(possible_slots[0]))
        } else {
            println!(
                "Where to split? (Enter '{}' or a time like '{}' or just '{}'. Leave {} for 'now'.)",
                "now".bright_blue(),
                "18:10".bright_blue(),
                "18".bright_blue(),
                "empty".bright_blue()
            );
            for s in possible_slots.iter() {
                println!(" - {}", Slot(*s).to_string().bright_blue());
            }
            get_input::<String>().and_then(|s| Slot::try_from(s).ok())
        };
        if let Some(choice) = choice {
            if possible_slots.contains(&choice) {
                self.ask_about_activity(now_or_last_entry, choice);
                if !only_one_split {
                    self.ask_about_activity(choice, Slot::now().next());
                }
            } else {
                println!("{}", "Invalid input!".red());
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
        let mut print = false;
        for date in dates {
            let time = date.borrow();
            let file = self.settings.get_filename_by_date(
                time.year() as usize,
                time.month() as usize,
                time.day() as usize,
            );
            if file.exists() {
                print = true;
                let day: Day = serde_json::from_str(
                    fs::read_to_string(file)
                        .expect("could not read file")
                        .as_str(),
                )
                .unwrap();
                if print_days && print {
                    println!(
                        "{}, {:02}.{:02}.: {:4.1} hrs. {}",
                        time.weekday().to_string(),
                        time.day(),
                        time.month(),
                        day.hours_productive(),
                        day.activity_string(&self.settings, step_by)
                    );
                }
                days.push(day);
            } else if print {
                println!(
                    "{}, {:02}.{:02}.:  no data",
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

        println!("Aggregated statistics from the last {} days:", days.len());
        println!("Hours Productive: {}", hours);
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
        println!("{}", "Saved!".bright_blue());
        self.day.write(&self.file);
    }
}

fn get_or_create_settings() -> Option<Settings> {
    let mut settings_file: PathBuf;

    if let Ok(path) = std::env::var(CONFIG_OVERRIDE_ENV_VAR) {
        settings_file = PathBuf::from(path)
    } else {
        settings_file = get_base_dirs()
            .config_dir()
            .join(CONFIG_FILENAME.to_string());
    }

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

        let author = Command::new(&settings.git)
            .arg("config")
            .arg("--global")
            .arg("user.name")
            .output()
            .ok()
            .map(|output| output.stdout)
            .filter(|output| output.len() > 1)
            .map(|result| String::from_utf8_lossy(&result[..result.len()-1]).to_string());
        if let Some(author) = author {
            settings.git_author = author;
        }

        let settings_str = toml::to_string(&settings).expect("seriaize");
        fs::write(&settings_file, settings_str).expect("write");
        println!("I have created a new config file here: {:?}", settings_file);
        println!("Please edit it and restart the program! :)");
        return None;
    }

    Some(toml::from_str(
        fs::read_to_string(&settings_file)
            .expect("read settings")
            .as_str(),
    )
    .expect(&format!("parse settings {:?}", &settings_file)))
}

fn main() {
    let settings = get_or_create_settings();
    if settings.is_none() {
        return;
    }
    let settings = settings.unwrap();

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
                println!("\tlastday (ld): Print statistics for the last day.");
                println!("\tedit (e): Edit activities for today in text editor.");
                println!("\tpath (p): Print today's data file path.");
                println!("\tsplit (s): Split the time since the last recorded activity in two.");
                println!("\ttoday (t): Print statistics for today.");
                println!("\tuntil (u): Like split, but only enter the first activity.");
                println!("\tweek (w, 2w, 3w): Print statistics for last seven, 14, 21 days.");
                println!("\tyear (y): Print statistics for last year.");
                println!();
                println!("Current data file: {:?}", &file);
                let settings_file = get_base_dirs()
                    .config_dir()
                    .join(CONFIG_FILENAME.to_string());
                println!("Config file: {:?}", &settings_file);
                println!("Set {} to override config file path.", CONFIG_OVERRIDE_ENV_VAR);
            },
            "p" | "path" => {
                println!("{}", file.display());
            }
            "a" | "activity" => {
                ui.print_current_slot_info();
                if let Some((start, end)) = ui.ask_about_start_and_end_time() {
                    ui.ask_about_activity(start, end);
                }
            },
            "d" | "day" => {
                let file = ui.ask_about_day();
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
                let file = settings.get_filename_by_date(time.year() as usize, time.month() as usize, time.day() as usize);
                println!("Loading file {:?}", file);
                let day: Day = serde_json::from_str(
                    fs::read_to_string(file)
                        .expect("could not read file")
                        .as_str(),
                )
                    .unwrap();
                day.print_stats(false, true);
            },
            "ld" | "lastday" => {
                let time = Local::now() - Duration::hours((*DAY_START / SLOTS_PER_HOUR) as i64) - Duration::days(1);
                let year = time.year() as usize;
                let month = time.month() as usize;
                let mut day = time.day() as usize;

                let mut file = None;
                while day > 1 {
                    let file_path = settings.get_filename_by_date(year, month, day);
                    if file_path.exists() {
                        file = Some(file_path);
                        break;
                    }
                    day -= 1;
                }

                if let Some(file_path) = file {
                    println!("Loading file {:?}", file_path);
                    println!("Last day: {}-{}-{}", year, month, day);
                    let day: Day = serde_json::from_str(
                        fs::read_to_string(file_path)
                            .expect("could not read file")
                            .as_str(),
                    )
                        .unwrap();
                    day.print_stats(false, true);
                } else {
                    println!("{}", "No data file found in this month.".red());
                }
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
                    true,
                );
            },
            "e" | "edit" => {
                let file = ui.ask_about_day();
                println!("Loading file {:?}", file);
                let day: Day = serde_json::from_str(
                    fs::read_to_string(file.clone())
                        .expect("could not read file")
                        .as_str(),
                )
                .unwrap();
                ui = UI {
                    day,
                    file,
                    settings: &settings,
                };
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
                        let file = settings.get_filename_by_date(
                            time.year() as usize,
                            time.month() as usize,
                            time.day() as usize,
                        );
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
}
