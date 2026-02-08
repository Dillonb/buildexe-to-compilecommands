use std::{collections::HashMap, fs, mem, path::PathBuf};
use regex::Regex;

struct RawCommand {
    thread: String,
    dir: PathBuf,
    lines: Vec<String>,
}

impl RawCommand {
    fn full_command(&self) -> String {
        self.lines.join(" ")
    }

    fn source_files(&self) -> Vec<String> {
        let mut source_files = Vec::new();
        for line in &self.lines {
            for token in line.split_whitespace() {
                if token.ends_with(".cpp") || token.ends_with(".c") {
                    source_files.push(token.to_string());
                }
            }
        }
        source_files
    }
}

struct CompileCommandsEntry {
    directory: PathBuf,
    command: String,
    file: String,
}

fn get_raw_commands(log: String) -> Vec<RawCommand> {
    let mut raw_commands : Vec<RawCommand> = Vec::new();

    let dir_regexes = vec![
        Regex::new(r"^(\d{4})>BUILDMSG: Processing (.+)$").unwrap(),
        Regex::new(r"^(\d{4})>Compiling (.+) \*+$").unwrap(),
     ];

    let mut dirs : HashMap<String, PathBuf> = HashMap::new();

    enum State {
        LookingForCommand,
        ReadingCommand,
    }
    let command_re = Regex::new(r"^(\d{4})>cl\s").unwrap();
    let mut state = State::LookingForCommand;
    let mut cur_command = Vec::new();
    let mut command_prefix = String::new();
    let mut cur_thread = String::new();
    for line in log.lines() {
        match state {
            State::LookingForCommand => {
                // Does this line begin a compilation command?
                if let Some(caps) = command_re.captures(line) {
                    let thread = caps.get(1).unwrap().as_str();
                    cur_thread = thread.to_string();
                    command_prefix = format!("{}>   ", thread);
                    cur_command.push(line[5..].trim().to_string());
                    state = State::ReadingCommand;
                } else {
                    // Check for messages that indicate a thread is processing a directory
                    for dir_regex in &dir_regexes {
                        if let Some(caps) = dir_regex.captures(line) {
                            let number = caps.get(1).unwrap().as_str();
                            let dir = caps.get(2).unwrap().as_str();
                            dirs.insert(number.to_string(), PathBuf::from(dir));
                            break;
                        }
                    }
                }
            }

            State::ReadingCommand => {
                if line.starts_with(&command_prefix) {
                    cur_command.push(line[5..].trim().to_string());
                } else {
                    let cur_dir = dirs.get(&cur_thread).expect(format!("Unable to determine directory for thread {}", cur_thread).as_str());
                    raw_commands.push(RawCommand {
                        thread: cur_thread.clone(),
                        dir: cur_dir.clone(),
                        lines: mem::replace(&mut cur_command, Vec::new()),
                    });
                    state = State::LookingForCommand;
                }
            }
        }

    }
    raw_commands
}

fn main() {
    let log = fs::read_to_string("buildfre.log").expect("Failed to read buildfre.log");

    let raw_commands = get_raw_commands(log);

    let compile_commands : Vec<CompileCommandsEntry> = raw_commands.iter().flat_map(|command| {
        let full_command = command.full_command();
        let source_files = command.source_files();
        // TODO: file should be resolved to an absolute path
        source_files.into_iter().map(move |source_file| CompileCommandsEntry {
            directory: command.dir.clone(),
            command: full_command.clone(),
            file: source_file,
        })
    }).collect();

    for command in compile_commands {
        println!("Directory: {}", command.directory.display());
        println!("File: {}", command.file);
    }
}
