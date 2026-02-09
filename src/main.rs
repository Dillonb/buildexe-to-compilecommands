use regex::Regex;
use std::{
    collections::HashMap,
    env, fs, mem,
    path::{self, PathBuf},
};

struct RawCommand {
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

#[derive(serde::Serialize, serde::Deserialize)]
struct CompileCommandsEntry {
    directory: PathBuf,
    command: String,
    file: String,
}

impl CompileCommandsEntry {
    fn from_raw_command(command: &RawCommand) -> impl Iterator<Item = CompileCommandsEntry> {
        let full_command = command.full_command();
        let source_files = command.source_files();
        source_files.into_iter().map(move |source_file| {
            let joined = command.dir.join(&source_file);
            let absolute = path::absolute(&joined)
                .expect(format!("Failed to resolve path for {}", joined.display()).as_str())
                .to_string_lossy()
                .to_string();
            CompileCommandsEntry {
                directory: command.dir.clone(),
                command: full_command.clone(),
                file: absolute,
            }
        })
    }
}

fn get_raw_commands(log: String) -> Vec<RawCommand> {
    let mut raw_commands: Vec<RawCommand> = Vec::new();

    let dir_regexes = vec![
        Regex::new(r"^(\d{4})>BUILDMSG: Processing (.+)$").unwrap(),
        Regex::new(r"^(\d{4})>Compiling (.+) \*+$").unwrap(),
    ];

    let mut dirs: HashMap<String, PathBuf> = HashMap::new();

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
                    let cur_dir = dirs.get(&cur_thread).expect(
                        format!("Unable to determine directory for thread {}", cur_thread).as_str(),
                    );
                    raw_commands.push(RawCommand {
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

fn merge_new_compile_commands(
    existing: Vec<CompileCommandsEntry>,
    new: Vec<CompileCommandsEntry>,
) -> Vec<CompileCommandsEntry> {
    let mut by_file: HashMap<String, CompileCommandsEntry> = HashMap::new();
    // Add existing to the map before new, so that new commands will overwrite existing ones for
    // the same file
    // This also works to deduplicate
    for command in existing.into_iter().chain(new.into_iter()) {
        // TODO: also check if the file exists on disk to remove stale entries
        by_file.insert(command.file.clone(), command);
    }
    by_file.into_values().collect()
}

fn main() {
    let args = env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        eprintln!("Usage: {} <path to buildfre.log>", args[0]);
        std::process::exit(1);
    }
    let log_path = &args[1];
    let absolute_log_path = path::absolute(log_path)
        .expect(format!("Failed to resolve path for {}", log_path).as_str());
    let dir_containing_log = absolute_log_path.parent().expect(
        format!(
            "Failed to get parent directory of {}",
            absolute_log_path.display()
        )
        .as_str(),
    );
    let compile_commands_path = dir_containing_log.join("compile_commands.json");
    let log = fs::read_to_string(log_path)
        .expect(format!("Failed to read build.exe log from {}", log_path).as_str());

    let raw_commands = get_raw_commands(log);

    let compile_commands: Vec<CompileCommandsEntry> = raw_commands
        .iter()
        .flat_map(CompileCommandsEntry::from_raw_command)
        .collect();

    // Read in the existing compile commands, if it exists, and merge with the new commands
    let existing_commands: Vec<CompileCommandsEntry> = if compile_commands_path.exists() {
        let existing_json = fs::read_to_string(&compile_commands_path).expect(
            format!(
                "Failed to read existing compile commands from {}",
                compile_commands_path.display()
            )
            .as_str(),
        );
        serde_json::from_str(&existing_json).expect(
            format!(
                "Failed to parse existing compile commands from {}",
                compile_commands_path.display()
            )
            .as_str(),
        )
    } else {
        Vec::new()
    };

    println!(
        "There are {} existing compile commands and {} new compile commands",
        existing_commands.len(),
        compile_commands.len()
    );

    let compile_commands = merge_new_compile_commands(existing_commands, compile_commands);

    // Write the compile commands to a JSON file
    let json = serde_json::to_string_pretty(&compile_commands)
        .expect("Failed to serialize compile commands to JSON");
    fs::write(&compile_commands_path, json).expect(
        format!(
            "Failed to write compile commands to {}",
            compile_commands_path.display()
        )
        .as_str(),
    );
    println!(
        "Successfully wrote compile commands to {}",
        compile_commands_path.display()
    );
}
