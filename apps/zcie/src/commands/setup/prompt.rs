//! Questions of the setup wizard on stdin/stdout. An empty answer or the end of the input takes
//! the default, so the wizard also works with piped answers.

use std::io::{BufRead, Write};

/// Wrong answers to one question before the default is taken.
const MAX_TRIES: usize = 3;

/// Reads answers from stdin.
pub struct Prompter {
    lines: std::io::Lines<std::io::StdinLock<'static>>,
}

impl Prompter {
    /// Makes a prompter on stdin.
    pub fn new() -> Self {
        Self { lines: std::io::stdin().lock().lines() }
    }

    fn read(&mut self, question: &str, default: &str) -> String {
        print!("{question} [{default}]: ");
        let _ = std::io::stdout().flush();
        match self.lines.next() {
            Some(Ok(line)) if !line.trim().is_empty() => line.trim().to_string(),
            Some(_) => default.to_string(),
            None => {
                println!();
                default.to_string()
            }
        }
    }

    /// Asks for free text.
    pub fn text(&mut self, question: &str, default: &str) -> String {
        self.read(question, default)
    }

    /// Asks for one of `options` (value, label). The answer is the number or the value.
    pub fn choose(&mut self, question: &str, options: &[(&str, &str)], current: &str) -> String {
        println!("\n{question}");
        for (i, (value, label)) in options.iter().enumerate() {
            let mark = if *value == current { "*" } else { " " };
            println!("  {mark} {}) {label}", i + 1);
        }
        let default = options.iter().position(|(v, _)| *v == current).map_or(1, |i| i + 1).to_string();
        for _ in 0..MAX_TRIES {
            let answer = self.read("  →", &default);
            let by_number = answer.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|i| options.get(i));
            let by_value = options.iter().find(|(v, _)| v.eq_ignore_ascii_case(&answer));
            if let Some((value, _)) = by_number.or(by_value) {
                return value.to_string();
            }
        }
        current.to_string()
    }

    /// Asks a yes/no question.
    pub fn yes_no(&mut self, question: &str, default: bool) -> bool {
        let hint = if default { "E/h, Y/n" } else { "e/H, y/N" };
        for _ in 0..MAX_TRIES {
            let answer = self.read(&format!("{question} ({hint})"), if default { "y" } else { "n" }).to_lowercase();
            match answer.as_str() {
                "y" | "yes" | "e" | "evet" => return true,
                "n" | "no" | "h" | "hayir" | "hayır" => return false,
                _ => {}
            }
        }
        default
    }
}
