//! `kition-cad` reads and writes only inside its workspace, and only through its host (KHP A28,
//! A29, A37, A42, A45, section 10.6): whatever an agent runs, and whatever a drawing names,
//! nothing appears in the process's current, home or temporary folder, nothing outside the
//! workspace is read, listed, changed or fetched, no answer tells whether a file outside exists,
//! and no connection is made. A workspace escape audit
//! found by reading code that the command line ran file commands written with a prefix
//! (`_RECOVER /x` read a whole file anywhere and told whether it was there), and that
//! `stylesmanager` read and wrote plot style tables by path inside `model.batch`.
//!
//! The test runs itself again as a child process whose current folder, `HOME` and `TMPDIR` are
//! empty scratch folders, beside a planted `outside/` folder of sentinel files: the agent names
//! them directly, through `model.batch` (no origin, the agent's, a person's the caller claims),
//! through the command line (plain, `_`, `.`, `-` and `'` prefixes, typed while another command
//! runs), and from drawings whose external references, images and text styles name them. Every
//! command the kit offers then runs with empty parameters. A localhost server a drawing names
//! must never be asked, and named pipes planted outside show any attempt to read them, even one
//! whose answer says nothing (the command line's RECOVER read a file and only echoed the result).
//!
//! The drawings' links are data (KHP A42): inspect reports every external reference and image
//! with whether the host lends its file, and only files inside the workspace are there.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_cad_doc::geom::{Vec2, Vec3};
use kition_cad_doc::{Common, Drawing, EntityKind, Image, Line, Space, TextStyle};
use kition_cad_kit::CadDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::ScreenshotScope;
use kition_kit_server::protocol::types::Origin;
use kition_kit_server::{KitBackend, OpenRequest, ScreenshotRequest};
use serde_json::{Map, Value, json};

/// Set in the child: the folder holding `workspace/`, `outside/`, and the scratch folders.
const CHILD: &str = "KITION_CAD_TEST_SANDBOX";
/// Set in the child: the port of the server no drawing may reach.
const PORT: &str = "KITION_CAD_TEST_PORT";
/// Text only files outside the workspace hold: it must reach no answer.
const SECRET: &str = "SENTINEL-c4d9";
/// A file outside the workspace that no call names: only a listing could find it.
const LISTED_ONLY: &str = "listed-only-c4d9.txt";
/// The colour of the picture outside the workspace.
const OUTSIDE: [u8; 3] = [23, 211, 97];

/// Every file under `dir`, relative to it, with its bytes and modification time (folders: empty).
fn tree(dir: &Path) -> BTreeMap<String, (Vec<u8>, Option<std::time::SystemTime>)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                stack.push(path.clone());
            }
            let bytes = if meta.is_file() { std::fs::read(&path).unwrap_or_default() } else { Vec::new() };
            out.insert(path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().into_owned(), (bytes, meta.modified().ok()));
        }
    }
    out
}

fn at(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// A protocol error as the agent reads it: its message and its data (the kit's detail).
fn error(error: &kition_kit_server::RpcError) -> String {
    format!("{error} {}", error.data.as_ref().map(Value::to_string).unwrap_or_default())
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// A one-colour PNG.
fn png(rgb: [u8; 3]) -> Vec<u8> {
    let picture = image::RgbImage::from_pixel(8, 8, image::Rgb(rgb));
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Whether a PNG shows a pixel of `rgb`.
fn shows(png: &[u8], rgb: [u8; 3]) -> bool {
    let Ok(picture) = image::load_from_memory(png) else { return false };
    picture.to_rgba8().pixels().any(|pixel| pixel[3] > 200 && (0..3).all(|c| pixel[c].abs_diff(rgb[c]) <= 3))
}

/// `text` decoded as base64, when it is.
fn base64(text: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if text.len() < 16 || !text.len().is_multiple_of(4) {
        return None;
    }
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = TABLE.iter().position(|known| *known == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// A drawing whose external references, images and text style name `targets` (one of each per
/// target), with a line so it has extents.
fn linking(targets: &[String]) -> Vec<u8> {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(100.0, 60.0, 0.0) })).unwrap();
    for (index, target) in targets.iter().enumerate() {
        let name = format!("Ref{index}");
        let mut block = kition_cad_doc::Block::new(&name);
        block.xref_path = Some(target.clone());
        d.blocks.insert(name.clone(), Arc::new(block));
        let x = 10.0 * index as f64;
        let insert = serde_json::from_value(json!({ "block": name, "insert": [x, 10.0, 0.0] })).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::Insert(insert)).unwrap();
        let image = Image {
            insert: Vec3::new(x, 30.0, 0.0),
            u: Vec3::new(1.0, 0.0, 0.0),
            v: Vec3::new(0.0, 1.0, 0.0),
            size: Vec2::new(8.0, 8.0),
            path: target.replace(".dxf", ".png"),
            name: format!("Picture{index}"),
            ..Image::default()
        };
        d.add(&Space::Model, Common::default(), EntityKind::Image(image)).unwrap();
    }
    if let Some(font) = targets.first() {
        d.text_styles.push(TextStyle { name: "Linked".into(), font: font.replace(".dxf", ".ttf"), ..TextStyle::default() });
        let mut text: kition_cad_doc::Text = serde_json::from_value(json!({ "insert": [10.0, 50.0, 0.0], "height": 2.5, "value": "Label" })).unwrap();
        text.style = "Linked".into();
        d.add(&Space::Model, Common::default(), EntityKind::Text(text)).unwrap();
    }
    kition_cad_io::write_dxf(&d).into_bytes()
}

/// The agent's session: the kit as kition-headless serves it, on a host folder (the workspace).
struct Agent {
    kit: Headless<CadDocument>,
    folder: HostFolder,
    /// Every answer, as text, and the bytes its base64 strings decode to.
    seen: Vec<Vec<u8>>,
    problems: Vec<String>,
}

impl Agent {
    fn new(workspace: &Path) -> Self {
        let mut kit = Headless::<CadDocument>::with_replica_port(
            KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_cad_kit::EDITOR_STATE).unwrap(),
        );
        let folder = HostFolder::new(workspace).unwrap();
        kit.set_host(folder.host());
        Self { kit, folder, seen: Vec::new(), problems: Vec::new() }
    }

    fn open(&mut self, name: &str) -> String {
        let file = self.folder.file(name).unwrap();
        self.kit.open(OpenRequest { file: &file, read_only: false, create: None }).unwrap().id
    }

    fn record(&mut self, value: &Value) {
        match value {
            Value::String(text) => {
                self.seen.push(text.as_bytes().to_vec());
                if let Some(bytes) = base64(text) {
                    self.seen.push(bytes);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| self.record(item)),
            Value::Object(fields) => fields.values().for_each(|item| self.record(item)),
            other => self.seen.push(other.to_string().into_bytes()),
        }
    }

    fn answer(&mut self, answer: Result<Value, String>) -> String {
        match &answer {
            Ok(value) => {
                self.record(value);
                value.to_string()
            }
            Err(message) => {
                self.record(&json!(message));
                format!("error: {message}")
            }
        }
    }

    /// `command` as `commands.execute` runs it for the agent.
    fn run(&mut self, id: &str, command: &str, value: Value) -> String {
        let answer = self.kit.execute(id, command, params(value), Origin::Agent);
        let answer = answer.map(|done| done.result.unwrap_or(Value::Null)).map_err(|e| error(&e));
        self.answer(answer)
    }

    /// `command` as a `command` operation of `model.batch`, with the batch's `origin`.
    fn batch(&mut self, id: &str, command: &str, value: Value, origin: &Value) -> String {
        let mut request = json!({ "documentId": id, "ops": [{ "op": "command", "id": command, "params": value }] });
        if !origin.is_null() {
            request["origin"] = origin.clone();
        }
        let answer = self.kit.model("model.batch", request).map_err(|e| error(&e));
        self.answer(answer)
    }

    /// Every way the agent can run `command` with `value`: directly, and in `model.batch` with no
    /// origin, the agent's, and a person's the caller claims. The answers, in that order.
    fn every_way(&mut self, id: &str, command: &str, value: &Value) -> Vec<String> {
        let mut answers = vec![self.run(id, command, value.clone())];
        for origin in [Value::Null, json!({ "kind": "agent", "principal": "agent:test" }), json!({ "kind": "person", "principal": "person:local" })] {
            answers.push(self.batch(id, command, value.clone(), &origin));
        }
        answers
    }

    /// Command-line entries, one call each; then the command they started ends (`edit.redo` ends a
    /// running command first, as Escape does, and with nothing to redo changes nothing else).
    fn type_lines(&mut self, id: &str, lines: &[String]) -> Vec<String> {
        let mut answers = Vec::new();
        for line in lines {
            answers.push(self.run(id, "commandLine", json!({ "text": line })));
        }
        self.run(id, "edit.redo", json!({}));
        answers
    }

    /// A screenshot of document `id`.
    fn picture(&mut self, id: &str) -> Vec<u8> {
        let request = ScreenshotRequest { scope: ScreenshotScope::Canvas, max_size: 512, scale: 1.0, target: None };
        self.kit.screenshot(id, &request).map(|shot| shot.png).unwrap_or_default()
    }

    /// Nothing outside the workspace reached an answer: its files' text, a listing, a folder of
    /// this process.
    fn assert_nothing_leaked(&mut self, root: &Path) {
        let home = at(root, "home");
        let tmp = at(root, "tmp");
        for seen in &self.seen {
            let text = String::from_utf8_lossy(seen);
            for leak in [SECRET, LISTED_ONLY, home.as_str(), tmp.as_str()] {
                if text.contains(leak) {
                    let excerpt: String = text.chars().take(300).collect();
                    self.problems.push(format!("an answer holds `{leak}` from outside the workspace: {excerpt}"));
                }
            }
            if seen.starts_with(b"\x89PNG") && shows(seen, OUTSIDE) {
                self.problems.push("a picture the kit answered shows the picture outside the workspace".into());
            }
        }
        assert!(self.problems.is_empty(), "{:#?}", self.problems);
    }
}

/// Runs the named test again in a child process whose current folder, `HOME` and `TMPDIR` are
/// empty, then checks that they stayed empty and that `outside/` did not change.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("kition-cad-workspace-only-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["workspace", "cwd", "home", "tmp", "outside/sub"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let root = std::fs::canonicalize(root).unwrap();
        let outside = root.join("outside");
        // The sentinels: a drawing, a plot style table, a picture, a font name, a listed-only file.
        let mut maker = CadDocument::create("secret.dxf", 0, 0).unwrap();
        maker.execute("text", json!({ "at": [10, 10], "height": 5, "text": SECRET })).unwrap();
        std::fs::write(outside.join("secret.dxf"), maker.save(Some("secret.dxf")).unwrap()).unwrap();
        maker.execute("stylesmanager", json!({ "action": "new", "name": "secret.ctb", "description": SECRET })).unwrap();
        let table = maker.execute("stylesmanager", json!({ "action": "save", "name": "secret.ctb" })).unwrap();
        std::fs::write(outside.join("secret.ctb"), table["text"].as_str().unwrap()).unwrap();
        std::fs::write(outside.join("secret.png"), png(OUTSIDE)).unwrap();
        std::fs::write(outside.join("secret.ttf"), SECRET).unwrap();
        std::fs::write(outside.join(LISTED_ONLY), SECRET).unwrap();
        std::fs::write(outside.join("sub/notes.txt"), SECRET).unwrap();
        // Pipes a probe names in place of a file: opening one to read is seen by `run_child`.
        for pipe in PIPES {
            let made = std::process::Command::new("mkfifo").arg(outside.join(pipe)).status().unwrap();
            assert!(made.success(), "mkfifo {pipe}");
        }
        Self { root }
    }

    fn workspace(&self) -> PathBuf {
        self.root.join("workspace")
    }

    fn outside(&self) -> PathBuf {
        self.root.join("outside")
    }

    fn run_child(&self, test: &str, port: u16, expected: &[&str]) {
        let planted = tree(&self.outside());
        let watcher = PipeWatcher::start(&self.outside());
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .current_dir(self.root.join("cwd"))
            .env(CHILD, &self.root)
            .env(PORT, port.to_string())
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", self.root.join("tmp"))
            .env("XDG_CONFIG_HOME", self.root.join("home/.config"))
            .env("XDG_DATA_HOME", self.root.join("home/.local/share"))
            .env("XDG_CACHE_HOME", self.root.join("home/.cache"))
            .env_remove("KITION_CAD_FONTALT")
            .env_remove("KITION_CAD_FONTFALLBACK")
            .status()
            .unwrap();
        let mut wrong = Vec::new();
        let opened = watcher.stop();
        if !opened.is_empty() {
            wrong.push(format!("the kit opened files outside the workspace to read them: {opened:?}"));
        }
        if !status.success() {
            wrong.push(format!("the agent's session reached outside the workspace (see its output above): {status}"));
        }
        for dir in ["cwd", "home", "tmp"] {
            let found: Vec<String> = tree(&self.root.join(dir)).into_keys().collect();
            if !found.is_empty() {
                wrong.push(format!("the kit wrote into its {dir} folder: {found:?}"));
            }
        }
        let now = tree(&self.outside());
        if now != planted {
            let before: Vec<&String> = planted.keys().collect();
            let after: Vec<&String> = now.keys().collect();
            wrong.push(format!("the folder outside the workspace changed (names, bytes or times): before {before:?}, after {after:?}"));
        }
        let found: Vec<String> = tree(&self.workspace()).into_keys().collect();
        if found != expected {
            wrong.push(format!("the workspace holds {found:?}, not only what the host saved and the test wrote ({expected:?})"));
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Named pipes outside the workspace that probes name instead of a file: whatever opens one to read
/// it is seen ([`PipeWatcher`]), even when the answer says nothing about it.
const PIPES: [&str; 2] = ["pipe.dxf", "pipe.ctb"];

/// Watches [`PIPES`]: a reader that opens one is let through (the pipe is opened to write and
/// closed, so the reader gets an empty file and goes on) and recorded.
struct PipeWatcher {
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::JoinHandle<Vec<String>>,
}

impl PipeWatcher {
    fn start(outside: &Path) -> Self {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "macos")]
        const O_NONBLOCK: i32 = 0x0004;
        #[cfg(not(target_os = "macos"))]
        const O_NONBLOCK: i32 = 0o4000;
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pipes: Vec<PathBuf> = PIPES.iter().map(|pipe| outside.join(pipe)).collect();
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut opened = Vec::new();
            while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
                for pipe in &pipes {
                    // Opening a pipe to write without waiting succeeds only while a reader has it open.
                    if std::fs::OpenOptions::new().write(true).custom_flags(O_NONBLOCK).open(pipe).is_ok() {
                        opened.push(pipe.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default());
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            opened
        });
        Self { stop, thread }
    }

    /// The pipes a reader opened, once each time.
    fn stop(self) -> Vec<String> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.thread.join().unwrap()
    }
}

/// The answers to `existing` and `missing`, the same probes naming a file outside that is there
/// and one that is not, must be the same once the names are taken out: no answer may tell whether
/// a file outside the workspace exists.
fn no_existence_oracle(agent: &mut Agent, what: &str, existing: &[String], missing: &[String], names: [(&str, &str); 2]) {
    let plain = |answers: &[String], from: &str| answers.iter().map(|answer| answer.replace(from, "<file>")).collect::<Vec<_>>();
    let (a, b) = (plain(existing, names[0].0), plain(missing, names[1].0));
    let (a, b) = (plain(&a, names[0].1), plain(&b, names[1].1));
    if a != b {
        agent.problems.push(format!("{what}: the answers tell whether a file outside the workspace exists:\n  there:   {a:?}\n  missing: {b:?}"));
    }
}

/// What the agent runs that names files outside the workspace, every way it can.
fn act_commands(root: &Path) {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    std::fs::write(workspace.join("basic.dxf"), CadDocument::create("basic.dxf", 0, 0).unwrap().save(Some("basic.dxf")).unwrap()).unwrap();
    let mut agent = Agent::new(&workspace);
    let id = agent.open("basic.dxf");
    agent.run(&id, "stylesmanager", json!({ "action": "new", "name": "agent.ctb" }));

    // The same probes against a file outside that is there and one that is not.
    let probes = |file: &str, made: &str| -> Vec<(&'static str, Value)> {
        let (file, made) = (at(&outside, file), at(&outside, made));
        vec![
            ("stylesmanager", json!({ "action": "load", "path": file.replace(".dxf", ".ctb") })),
            ("stylesmanager", json!({ "action": "save", "name": "agent.ctb", "path": made.replace(".dxf", ".ctb") })),
            ("open", json!({ "path": file })),
            ("recover", json!({ "path": file })),
            ("insert", json!({ "name": "S", "file": file, "at": [0, 0] })),
            ("qsave", json!({ "path": made })),
            ("saveas", json!({ "path": made })),
            ("wblock", json!({ "path": made, "handles": [] })),
            ("plot", json!({ "path": made.replace(".dxf", ".pdf") })),
            ("exportpdf", json!({ "path": made.replace(".dxf", ".pdf") })),
            ("commandLine", json!({ "text": format!("stylesmanager {}", json!({ "action": "load", "path": file.replace(".dxf", ".ctb") })) })),
            ("commandLine", json!({ "text": format!("recover {}", json!({ "path": file })) })),
        ]
    };
    let lines = |file: &str| -> Vec<Vec<String>> {
        let file = at(&outside, file);
        let ctb = file.replace(".dxf", ".ctb");
        let load = json!({ "action": "load", "path": ctb });
        vec![
            vec![format!("_RECOVER {file}")],
            vec!["_recover".into(), file.clone()],
            vec![format!(".recover \"{file}\"")],
            vec![format!("-recover {file}")],
            vec![format!("'recover {file}")],
            vec!["line".into(), "0,0".into(), format!("'_recover {file}")],
            vec![format!("_open {file}")],
            vec![format!("_insert S={file} 0,0 1 1 0")],
            vec![format!("-insert {file} 0,0 1 1 0")],
            vec![format!("_stylesmanager {load}")],
            vec![format!(".stylesmanager {load}")],
            vec![format!("_saveas {file}")],
            vec![format!("_wblock {file}")],
            vec!["_qsave".into()],
            vec!["_plot".into()],
        ]
    };
    for ((command, there), (_, absent)) in probes("secret.dxf", "made.dxf").into_iter().zip(probes("absent.dxf", "made.dxf")) {
        let a = agent.every_way(&id, command, &there);
        let b = agent.every_way(&id, command, &absent);
        no_existence_oracle(&mut agent, &format!("{command} {there}"), &a, &b, [("secret", "absent"), ("secret", "absent")]);
    }
    for (there, absent) in lines("secret.dxf").into_iter().zip(lines("absent.dxf")) {
        let a = agent.type_lines(&id, &there);
        let b = agent.type_lines(&id, &absent);
        no_existence_oracle(&mut agent, &format!("the command line {there:?}"), &a, &b, [("secret", "absent"), ("secret", "absent")]);
    }

    // The same probes naming pipes outside: whatever opens one to read it is seen.
    for (command, value) in probes("pipe.dxf", "made.dxf") {
        agent.every_way(&id, command, &value);
    }
    for entered in lines("pipe.dxf") {
        agent.type_lines(&id, &entered);
    }

    // Every command the kit offers the agent, with no parameters.
    let offered: Vec<String> = agent.kit.commands(Some(&id)).unwrap().into_iter().map(|command| command.id).collect();
    for command in offered {
        // These end or replace the drawing the rest of the sweep runs in.
        if matches!(command.as_str(), "close" | "closeall" | "new" | "document.switch") {
            continue;
        }
        agent.run(&id, &command, json!({}));
        agent.run(&id, "edit.redo", json!({}));
    }
    agent.kit.save(&id, None, None).unwrap();
    agent.assert_nothing_leaked(root);
}

#[test]
fn agent_commands_write_read_and_list_nothing_outside_the_workspace() {
    if let Some(root) = std::env::var_os(CHILD) {
        act_commands(&PathBuf::from(root));
        return;
    }
    let sandbox = Sandbox::new("commands");
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    sandbox.run_child("agent_commands_write_read_and_list_nothing_outside_the_workspace", server.local_addr().unwrap().port(), &["basic.dxf"]);
    let asked = server.accept();
    let quiet = matches!(&asked, Err(error) if error.kind() == std::io::ErrorKind::WouldBlock);
    assert!(quiet, "the kit made a connection (KHP A37): {asked:?}");
}

/// What the agent does with drawings that link files outside the workspace (and one inside).
fn act_documents(root: &Path) {
    let workspace = root.join("workspace");
    let mut agent = Agent::new(&workspace);
    let id = agent.open("hostile.dxf");
    let picture = agent.picture(&id);
    agent.seen.push(picture);
    agent.run(&id, "file.render", json!({}));
    for format in ["pdf", "svg", "png"] {
        agent.run(&id, "file.export", json!({ "format": format }));
        let name = format!("hostile.{format}");
        let _ = agent.kit.export(&id, &agent.folder.export(&name).unwrap(), format, None);
        agent.seen.push(std::fs::read(workspace.join(&name)).unwrap_or_default());
    }
    let checked = agent.kit.check(&id).unwrap();
    agent.record(&checked);
    let text = agent.kit.text(&id, 100_000).unwrap_or_default();
    agent.record(&text);
    let described = agent.kit.describe(&id).map(|described| Value::Object(described.state)).unwrap_or_default();
    agent.record(&described);

    // The links are data (KHP A42): every one is reported, those outside the workspace (or not a
    // workspace path) as not there, the drawing and picture beside the hostile drawing as there.
    let links = described.get("links").and_then(Value::as_array).cloned().unwrap_or_default();
    let inside: Vec<&Value> = links.iter().filter(|link| link["exists"] == json!(true)).collect();
    let targets: Vec<String> = links.iter().filter_map(|link| link["target"].as_str().map(str::to_owned)).collect();
    if links.len() != 12 || inside.len() != 2 || inside.iter().any(|link| link["path"].as_str().is_none_or(|path| !path.starts_with("inside."))) {
        agent.problems.push(format!("inspect should report 12 links (6 references, 6 pictures), 2 of them there: {links:#?}"));
    }
    if links.iter().any(|link| link["exists"] == json!(false) && link.get("path").is_some()) {
        agent.problems.push(format!("a link that is not there names a workspace path: {links:#?}"));
    }
    for kind in ["embed", "media"] {
        if links.iter().filter(|link| link["kind"] == kind).count() != 6 {
            agent.problems.push(format!("inspect should report 6 links of kind {kind}: {targets:?}"));
        }
    }
    // The check reports every reference whose drawing is not loaded.
    let unresolved = checked.to_string().matches("cad.unresolved-xref").count();
    if unresolved < 5 {
        agent.problems.push(format!("the check should report the references outside the workspace as cad.unresolved-xref: {checked}"));
    }
    agent.kit.save(&id, None, None).unwrap();
    agent.assert_nothing_leaked(root);
}

#[test]
fn drawings_and_links_read_nothing_outside_the_workspace() {
    if let Some(root) = std::env::var_os(CHILD) {
        act_documents(&PathBuf::from(root));
        return;
    }
    let sandbox = Sandbox::new("documents");
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let port = server.local_addr().unwrap().port();
    let (workspace, outside) = (sandbox.workspace(), sandbox.outside());
    std::fs::write(workspace.join("inside.dxf"), CadDocument::create("inside.dxf", 0, 0).unwrap().save(Some("inside.dxf")).unwrap()).unwrap();
    std::fs::write(workspace.join("inside.png"), png([200, 30, 30])).unwrap();
    let targets = [
        at(&outside, "secret.dxf"),
        format!("file://{}", at(&outside, "secret.dxf")),
        "../outside/secret.dxf".to_owned(),
        format!("http://127.0.0.1:{port}/secret.dxf"),
        format!("//127.0.0.1:{port}/share/secret.dxf"),
        "inside.dxf".to_owned(),
    ];
    std::fs::write(workspace.join("hostile.dxf"), linking(&targets)).unwrap();
    sandbox.run_child(
        "drawings_and_links_read_nothing_outside_the_workspace",
        port,
        &["hostile.dxf", "hostile.pdf", "hostile.png", "hostile.svg", "inside.dxf", "inside.png"],
    );
    let asked = server.accept();
    let quiet = matches!(&asked, Err(error) if error.kind() == std::io::ErrorKind::WouldBlock);
    assert!(quiet, "the kit connected to a server a drawing named (KHP A37): {asked:?}");
}
