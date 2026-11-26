//! What the editor's command line shows besides the prompt text: the prompt taken apart (its
//! message, options, default and what it asks for), the history the person's entries wrote, and
//! the command catalogue (`command.list`) the line completes names from and the ribbon starts
//! prompts by.
//!
//! The prompt and the history are in `inspect()` under `/drafting/commandLine`, inside the editor
//! state ([`crate::EDITOR_STATE`]) and out of what an agent reads; the catalogue is a read command
//! because it changes only with the engine.
use super::*;

/// The most lines the history keeps (the command line's history window shows them).
pub const HISTORY_LINES: usize = 200;
/// The most characters of one history line; a longer line is cut.
const LINE_CHARS: usize = 2000;
/// The read command that lists the commands the command line knows.
pub const LIST: &str = "command.list";

impl CadDocument {
    /// Runs one of the person's entries (a tool event, a command-line entry) and puts what the
    /// engine wrote meanwhile ("Command: LINE", a distance, "*Cancel*") on the command line's
    /// history. What other calls wrote before (an agent's commands) is not the person's and is
    /// dropped.
    pub(super) fn echoed<T>(&mut self, run: impl FnOnce(&mut Self) -> T) -> T {
        self.session.log.clear();
        let out = run(self);
        for line in std::mem::take(&mut self.session.log) {
            self.write_history(&line);
        }
        out
    }

    /// Adds a line to the command line's history.
    pub(super) fn write_history(&mut self, line: &str) {
        let line: String = line.chars().take(LINE_CHARS).collect();
        self.history.push_back(line);
        self.history_serial = self.history_serial.saturating_add(1);
        while self.history.len() > HISTORY_LINES {
            self.history.pop_front();
        }
    }

    /// The entry typed at a command's prompt, written to the history beside the prompt it answers
    /// ("LINE Specify next point or [Close/Undo]: 10,20"). Between commands the engine writes the
    /// command it starts ("Command: LINE") itself.
    pub(super) fn echo_entry(&mut self, text: &str) {
        if self.hot_grip.is_none() && self.session.running.is_none() {
            return;
        }
        let prompt = self.prompt();
        let text = text.trim();
        if text.is_empty() { self.write_history(&prompt) } else { self.write_history(&format!("{prompt} {text}")) }
    }

    /// `inspect()`'s `/drafting/commandLine`: the prompt taken apart and the history, its last
    /// [`HISTORY_LINES`] lines and how many lines it has ever had (`serial`, which tells the editor
    /// which lines are new).
    pub(super) fn command_line_state(&self) -> Value {
        json!({
            "prompt": self.prompt_parts(),
            "history": { "serial": self.history_serial, "lines": self.history },
        })
    }

    /// The prompt as the command line draws it: the command (as typed, in capitals), the message,
    /// the options (clicked, each is typed as written), the default (clicked, it is Enter), and
    /// what it asks for (`point`, `distance`, `angle`, `select`, `text`, `keyword`; `command`
    /// between commands), and its base point (`[x, y]`, the last point the rubber band and the
    /// dynamic input boxes measure from; `null` without one; not given between commands).
    fn prompt_parts(&self) -> Value {
        if let Some(parts) = self.grip_prompt_parts() {
            return parts;
        }
        match (&self.session.running, self.session.current_prompt()) {
            (Some(running), Some(prompt)) => {
                let mut parts = json!({
                    "command": kition_cad_engine::cmd::typed_name(&running.id).to_ascii_uppercase(),
                    "message": prompt.message,
                    "keywords": prompt.keywords,
                    "default": prompt.default,
                    "kind": prompt.kind(),
                    "base": prompt.base.filter(|base| base.is_finite()).map(|base| [base.x, base.y]),
                });
                // Where TEXT's and MTEXT's text goes: the editor types it there ([`in_place`]).
                if let Some(place) = &prompt.text_box {
                    parts["textBox"] = in_place::text_box(place);
                }
                parts
            }
            _ => json!({ "command": null, "message": "Command", "keywords": [], "default": null, "kind": "command" }),
        }
    }
}

/// `command.list`: the commands the command line knows, with the name each is typed by, its
/// aliases, its title, its menu path, whether it asks for its input in prompts (`interactive`) and
/// whether the command line may start it (`commandLine`: not a command that reads or writes
/// files). `search` keeps the commands whose id, name, title or an alias contains it (any case).
pub(super) fn list(params: &Value) -> Value {
    let search = params.get("search").and_then(Value::as_str).map(|text| text.trim().to_ascii_lowercase()).filter(|text| !text.is_empty());
    let found = |spec: &kition_cad_engine::CommandSpec| {
        search.as_deref().is_none_or(|text| {
            [spec.id, spec.typed, spec.label].iter().chain(spec.aliases.iter()).any(|word| word.to_ascii_lowercase().contains(text))
        })
    };
    let commands: Vec<Value> = kition_cad_engine::cmd::command_specs()
        .iter()
        .filter(|spec| found(spec))
        .map(|spec| {
            json!({
                "id": spec.id,
                "name": spec.typed,
                "aliases": spec.aliases,
                "title": spec.label,
                "menu": spec.menu,
                "interactive": spec.interactive.is_some(),
                "commandLine": command_line_gate(spec.id, None).is_ok(),
            })
        })
        .collect();
    json!({ "commands": commands })
}

/// The catalogue entry of [`LIST`].
pub(super) fn command() -> KitCommand {
    KitCommand {
        id: LIST.into(),
        title: "Command List".into(),
        params: Some("{search?: string}".into()),
        mutates: false,
        enabled: true,
        ..Default::default()
    }
    .with_kit_schema()
}
