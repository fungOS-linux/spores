//! doc-shell: a native window onto the document layer.
//!
//! Three panes — the vault, one document, the agent watching it — in one
//! arrangement that flips. The document layer is the model; every pane here is a
//! view of it, and none of them keeps a second copy.

use std::{path::PathBuf, process::ExitCode};

use gpui_kit::assets::Assets;
use gpui_kit::{
    App, AppContext as _, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use doc_shell::open_doc::View;
use doc_shell::shell::{
    CycleAppearance, FlipPanes, MoveSidebar, NextSession, PrevSession, Reload, SaveDocument, Shell,
    Start, ToggleView,
};
use doc_shell::theme::Appearance;

const USAGE: &str = "\
doc-shell — a native window onto the fungos-vfs document layer

    doc-shell [--root DIR] [--open PATH] [--view source|preview] [--flipped]
              [--actor NAME] [--title NAME]

    --root DIR      the vault: a directory of files, walked for documents
                    (default: the current directory)
    --open PATH     a file inside the vault to open at start
    --view VIEW     markdown opens as `preview` (rendered) or `source`
    --flipped       start with the document and the transcript swapped
    --appearance    light or dark (default: dark)
    --actor NAME    who edits and comments are attributed to (default: this
                    machine's user)
    --title NAME    what the vault is called in the tree and the toolbar

Records for a vault live in <root>/.doc-shell/, so a vault is one directory.
Keys: cmd-shift-p flip panes · cmd-shift-b move the vault · cmd-shift-e source or
preview · cmd-s save · ctrl-shift-<same> also works where cmd does not.
";

fn main() -> ExitCode {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("doc-shell: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let root = match std::fs::canonicalize(&options.root) {
        Ok(root) => root,
        Err(error) => {
            eprintln!(
                "doc-shell: {} is not a directory I can read: {error}",
                options.root.display()
            );
            return ExitCode::FAILURE;
        }
    };
    let title = options
        .title
        .clone()
        .or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "vault".to_owned());

    let app = gpui_kit::application().with_assets(Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);
        bind_keys(cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1440.), px(900.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("doc-shell".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let start = Start {
            open: options.open.as_deref(),
            flipped: options.flipped,
            view: options.view,
            appearance: options.appearance,
        };
        let actor = options.actor.clone();
        let root = root.clone();
        gpui_kit::open_window(window_options, cx, move |window, cx| {
            cx.new(|cx| Shell::new(&root, title.clone(), &actor, start, window, cx))
        })
        .expect("failed to open a window");
        cx.activate(true);
    });
    ExitCode::SUCCESS
}

/// One key per action, and the `ctrl-` spelling beside it: a window manager that
/// eats a chord should not be the reason a shortcut looks missing.
fn bind_keys(cx: &mut App) {
    macro_rules! bind {
        ($action:expr, $mac:literal, $ctrl:literal) => {
            cx.bind_keys([
                KeyBinding::new($mac, $action, None),
                KeyBinding::new($ctrl, $action, None),
            ]);
        };
    }
    bind!(FlipPanes, "cmd-shift-p", "ctrl-shift-p");
    bind!(MoveSidebar, "cmd-shift-b", "ctrl-shift-b");
    bind!(ToggleView, "cmd-shift-e", "ctrl-shift-e");
    bind!(SaveDocument, "cmd-s", "ctrl-s");
    bind!(Reload, "cmd-shift-r", "ctrl-shift-r");
    bind!(NextSession, "cmd-alt-right", "ctrl-alt-right");
    bind!(PrevSession, "cmd-alt-left", "ctrl-alt-left");
    bind!(CycleAppearance, "cmd-shift-t", "ctrl-shift-t");
}

#[derive(Clone, Debug)]
struct Options {
    root: PathBuf,
    open: Option<String>,
    view: View,
    flipped: bool,
    actor: String,
    title: Option<String>,
    appearance: Appearance,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            open: None,
            view: View::Preview,
            flipped: false,
            actor: whoami(),
            title: None,
            appearance: Appearance::default(),
        }
    }
}

impl Options {
    /// Parse the arguments. `Ok(None)` means the caller asked for the usage text.
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Option<Self>, String> {
        let mut options = Self::default();
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            let mut value = |flag: &str| -> Result<String, String> {
                arguments
                    .next()
                    .ok_or_else(|| format!("{flag} needs a value"))
            };
            match argument.as_str() {
                "-h" | "--help" => return Ok(None),
                "--root" => options.root = PathBuf::from(value("--root")?),
                "--open" => options.open = Some(value("--open")?),
                "--actor" => options.actor = value("--actor")?,
                "--title" => options.title = Some(value("--title")?),
                "--flipped" => options.flipped = true,
                "--view" => {
                    options.view = match value("--view")?.as_str() {
                        "source" => View::Source,
                        "preview" => View::Preview,
                        other => return Err(format!("--view is source or preview, not {other}")),
                    }
                }
                "--appearance" => {
                    let text = value("--appearance")?;
                    options.appearance = Appearance::parse(&text).ok_or_else(|| {
                        format!("--appearance is light, dark or system, not {text}")
                    })?;
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(Some(options))
    }
}

/// The acting identity when none was given: enough to attribute an edit to a
/// person, and stable across runs on one machine.
fn whoami() -> String {
    std::env::var("DOC_SHELL_ACTOR")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "local".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Option<Options>, String> {
        Options::parse(arguments.iter().map(|argument| (*argument).to_owned()))
    }

    #[test]
    fn the_defaults_are_the_current_directory_and_the_rendered_view() {
        let options = parse(&[]).unwrap().unwrap();
        assert_eq!(options.view, View::Preview);
        assert!(!options.flipped);
        assert!(options.open.is_none());
    }

    #[test]
    fn every_flag_is_read() {
        let options = parse(&[
            "--root",
            "/tmp/vault",
            "--open",
            "notes/today.md",
            "--view",
            "source",
            "--flipped",
            "--actor",
            "review",
            "--title",
            "Future Present Labs",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.root, PathBuf::from("/tmp/vault"));
        assert_eq!(options.open.as_deref(), Some("notes/today.md"));
        assert_eq!(options.view, View::Source);
        assert!(options.flipped);
        assert_eq!(options.actor, "review");
        assert_eq!(options.title.as_deref(), Some("Future Present Labs"));
    }

    #[test]
    fn a_bad_view_is_refused_rather_than_guessed() {
        let error = parse(&["--view", "wysiwyg"]).unwrap_err();
        assert!(error.contains("source or preview"), "{error}");
    }

    #[test]
    fn a_flag_without_a_value_is_refused() {
        assert!(parse(&["--root"]).is_err());
        assert!(parse(&["--nonsense"]).is_err());
    }

    #[test]
    fn help_is_not_an_error() {
        assert!(parse(&["--help"]).unwrap().is_none());
    }
}
