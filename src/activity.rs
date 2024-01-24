use serde_derive::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::fmt;
use colored::Colorize;
use crate::COLORS;
use crate::settings::Settings;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, Hash)]
pub struct Activity {
    pub name: String,
    pub productive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>
}

impl PartialEq for Activity {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Activity {
    pub fn get_by_name(actis: &[Activity], name: &str) -> Option<Self> {
        actis.iter().find(|o| o.name == name).cloned()
    }

    pub fn prompt(settings: &Settings) -> Option<&Activity> {
        let shortcuts = settings.get_shortcuts();
        settings.activities.iter().enumerate().for_each(|(i, o)| {
            let mut name = o.to_string();
            if let Some(chr) = &shortcuts[i] {
                name = name.replacen(*chr, &format!("[{}]", chr), 1);
            }
            println!("\t{}: {}", i, name);
        });
        let input = crate::get_input::<String>()?.trim().chars().next()?;
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

    pub fn color(&self) -> &'static str {
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
