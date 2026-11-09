//! Native file dialogs without linking a GUI toolkit.
//!
//! Windows uses the Win32 common dialogs in [`crate::app`]. macOS and Linux have
//! no toolkit bindings in this project, so they ask the platform's own dialog
//! through the standard command-line helper — `osascript` on macOS and `zenity`
//! or `kdialog` on Linux desktops. Each returns a real filesystem path, so
//! relative images, Save and "Save as" keep working exactly like on Windows.
//!
//! When no helper is installed the returned error says the dialog is *not
//! supported*, which is what makes the frontend fall back to the in-window
//! `<input type="file">` picker.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Dialog backends, split out so both can be tested on any host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogPlatform {
    MacOs,
    Linux,
}

/// One way of showing a native dialog: a program, its arguments, and the
/// signals that mean "the user cancelled" rather than "this failed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogCommand {
    pub program: &'static str,
    pub args: Vec<String>,
    pub cancel_codes: &'static [i32],
    pub cancel_markers: &'static [&'static str],
}

const MARKDOWN_EXTENSIONS: &str = "*.md *.markdown *.mdx *.mdown *.mkd *.txt";
const MARKDOWN_FILTER: &str = "Markdown (*.md *.markdown *.mdx *.mdown *.mkd *.txt)|*.md *.markdown *.mdx *.mdown *.mkd *.txt\nAll files (*)|*";

pub const fn current_platform() -> DialogPlatform {
    if cfg!(target_os = "macos") {
        DialogPlatform::MacOs
    } else {
        DialogPlatform::Linux
    }
}

pub fn open_commands(platform: DialogPlatform) -> Vec<DialogCommand> {
    match platform {
        DialogPlatform::MacOs => vec![DialogCommand {
            program: "osascript",
            args: vec![
                "-e".to_string(),
                "POSIX path of (choose file with prompt \"Open a Markdown document\")".to_string(),
            ],
            cancel_codes: &[],
            cancel_markers: &["user canceled", "(-128)"],
        }],
        DialogPlatform::Linux => vec![
            DialogCommand {
                program: "zenity",
                args: vec![
                    "--file-selection".to_string(),
                    "--title=Open a Markdown document".to_string(),
                    format!("--file-filter=Markdown | {MARKDOWN_EXTENSIONS}"),
                    "--file-filter=All files | *".to_string(),
                ],
                cancel_codes: &[1, 5],
                cancel_markers: &[],
            },
            DialogCommand {
                program: "kdialog",
                args: vec![
                    "--getopenfilename".to_string(),
                    start_dir(),
                    MARKDOWN_FILTER.to_string(),
                    "--title".to_string(),
                    "Open a Markdown document".to_string(),
                ],
                cancel_codes: &[1],
                cancel_markers: &[],
            },
        ],
    }
}

pub fn save_commands(platform: DialogPlatform, default_name: &str) -> Vec<DialogCommand> {
    let name = sanitise_name(default_name);
    match platform {
        DialogPlatform::MacOs => vec![DialogCommand {
            program: "osascript",
            args: vec![
                "-e".to_string(),
                format!(
                    "POSIX path of (choose file name with prompt \"Save Markdown document\" \
                     default name \"{}\")",
                    escape_applescript(&name)
                ),
            ],
            cancel_codes: &[],
            cancel_markers: &["user canceled", "(-128)"],
        }],
        DialogPlatform::Linux => vec![
            DialogCommand {
                program: "zenity",
                args: vec![
                    "--file-selection".to_string(),
                    "--save".to_string(),
                    "--confirm-overwrite".to_string(),
                    "--title=Save Markdown document".to_string(),
                    format!("--filename={name}"),
                    format!("--file-filter=Markdown | {MARKDOWN_EXTENSIONS}"),
                    "--file-filter=All files | *".to_string(),
                ],
                cancel_codes: &[1, 5],
                cancel_markers: &[],
            },
            DialogCommand {
                program: "kdialog",
                args: vec![
                    "--getsavefilename".to_string(),
                    PathBuf::from(start_dir()).join(&name).display().to_string(),
                    MARKDOWN_FILTER.to_string(),
                    "--title".to_string(),
                    "Save Markdown document".to_string(),
                ],
                cancel_codes: &[1],
                cancel_markers: &[],
            },
        ],
    }
}

/// Runs the first available dialog helper and returns the picked path.
/// `Ok(None)` means the user cancelled.
pub fn run(commands: &[DialogCommand]) -> Result<Option<PathBuf>, String> {
    let mut failures: Vec<String> = Vec::new();
    for command in commands {
        match Command::new(command.program).args(&command.args).output() {
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if output.status.success() {
                    return Ok(parse_path(&String::from_utf8_lossy(&output.stdout)));
                }
                if is_cancelled(command, output.status.code(), &stderr) {
                    return Ok(None);
                }
                failures.push(format!("{}: {stderr}", command.program));
            }
            // Helper not installed: try the next one.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("Could not start the file dialog: {err}")),
        }
    }

    if failures.is_empty() {
        Err(
            "A native file dialog is not supported on this system. Install zenity or kdialog, \
             or pick the file in the window instead."
                .to_string(),
        )
    } else {
        Err(format!("The file dialog failed ({}).", failures.join("; ")))
    }
}

pub fn parse_path(stdout: &str) -> Option<PathBuf> {
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    Some(PathBuf::from(line))
}

fn is_cancelled(command: &DialogCommand, code: Option<i32>, stderr: &str) -> bool {
    if let Some(code) = code {
        if command.cancel_codes.contains(&code) {
            return true;
        }
    }
    let lower = stderr.to_ascii_lowercase();
    command
        .cancel_markers
        .iter()
        .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
}

/// Keeps a suggested file name to its final component and strips control
/// characters, so a document name can never escape the dialog or the script.
fn sanitise_name(name: &str) -> String {
    let file_name = Path::new(name.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let cleaned: String = file_name.chars().filter(|ch| !ch.is_control()).collect();
    if cleaned.trim().is_empty() {
        "untitled.md".to_string()
    } else {
        cleaned
    }
}

fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn start_dir() -> String {
    dirs::home_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_open_falls_back_from_zenity_to_kdialog() {
        let commands = open_commands(DialogPlatform::Linux);
        assert_eq!(commands[0].program, "zenity");
        assert!(commands[0]
            .args
            .iter()
            .any(|arg| arg.contains("*.md") && arg.contains("Markdown")));
        assert_eq!(commands[1].program, "kdialog");
        assert_eq!(commands[1].args[0], "--getopenfilename");
    }

    #[test]
    fn macos_open_uses_the_native_panel_script() {
        let commands = open_commands(DialogPlatform::MacOs);
        assert_eq!(commands[0].program, "osascript");
        let script = &commands[0].args[1];
        assert!(script.contains("choose file"));
        assert!(script.contains("POSIX path of"));
    }

    #[test]
    fn save_commands_prefill_the_suggested_name() {
        let linux = save_commands(DialogPlatform::Linux, "changelog.md");
        assert!(linux[0].args.iter().any(|arg| arg == "--save"));
        assert!(linux[0]
            .args
            .iter()
            .any(|arg| arg == "--filename=changelog.md"));
        assert!(linux[1].args[1].ends_with("changelog.md"));

        let mac = save_commands(DialogPlatform::MacOs, "changelog.md");
        assert!(mac[0].args[1].contains("default name \"changelog.md\""));
    }

    #[test]
    fn suggested_names_cannot_escape_their_directory() {
        let commands = save_commands(DialogPlatform::Linux, "../etc/passwd");
        assert!(commands[0]
            .args
            .iter()
            .any(|arg| arg == "--filename=passwd"));
        assert!(!commands[0].args.iter().any(|arg| arg.contains("..")));

        let mac = save_commands(DialogPlatform::MacOs, "a \"quoted\" name.md");
        let script = &mac[0].args[1];
        assert!(script.contains("\\\"quoted\\\""));
        assert!(!script.contains("../"));

        assert_eq!(escape_applescript("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn empty_names_fall_back_to_untitled() {
        let commands = save_commands(DialogPlatform::Linux, "   ");
        assert!(commands[0]
            .args
            .iter()
            .any(|arg| arg == "--filename=untitled.md"));
    }

    #[test]
    fn dialog_output_is_trimmed_to_a_path() {
        assert_eq!(
            parse_path("  /home/me/notes/a b.md \r\n").unwrap(),
            PathBuf::from("/home/me/notes/a b.md")
        );
        assert!(parse_path("\n\n").is_none());
        assert!(parse_path("").is_none());
    }

    #[test]
    fn cancellation_is_told_apart_from_failure() {
        let zenity = &open_commands(DialogPlatform::Linux)[0];
        assert!(is_cancelled(zenity, Some(1), ""));
        assert!(is_cancelled(zenity, Some(5), ""));
        assert!(!is_cancelled(zenity, Some(9), "cannot open display"));

        let osascript = &open_commands(DialogPlatform::MacOs)[0];
        assert!(is_cancelled(
            osascript,
            Some(1),
            "execution error: User canceled. (-128)"
        ));
        assert!(!is_cancelled(
            osascript,
            Some(1),
            "execution error: Can't get file."
        ));
    }
}
