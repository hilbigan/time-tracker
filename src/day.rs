use serde_derive::{Deserialize, Serialize};
use std::path::Path;
use std::{fmt, fs, io};
use chrono::{Local, Timelike};
use std::convert::TryFrom;
use std::io::ErrorKind;
use std::ops::Deref;
use std::fmt::{Display, Formatter};
use colored::Colorize;
use itertools::Itertools;
use crate::activity::Activity;
use crate::{DAY_SLOTS, DAY_START, PRODUCTIVE_TARGET, SLOTS_PER_HOUR};
use crate::settings::Settings;

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

    pub fn next(&self) -> Slot {
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
pub struct Day {
    pub time_slots: Vec<Option<Activity>>,
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
                    // Split if activities are different or comments are different
                    if *act != o || act.as_ref().zip(o.as_ref()).map_or(false, |(a, b)| a.comment != b.comment) {
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

    pub fn print_stats(&self, with_current_time: bool, trim_start: bool) {
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
            "Hours Productive: {}",
            self.hours_productive()
        );
    }

    pub fn write(&self, path: &Path) {
        fs::write(path, serde_json::to_string(&self).unwrap()).expect("write failed");
    }
}
