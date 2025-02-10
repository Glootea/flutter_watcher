use notify::{Event, RecursiveMode, Result, Watcher};
use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{exit, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        mpsc::{self},
        Arc, Mutex,
    },
    thread::sleep,
    time::Duration,
};
use tokio::{spawn, task::JoinHandle};

#[tokio::main]
async fn main() {
    let args = env::args().into_iter().skip(1);
    let path = fs::canonicalize(env::current_dir().expect("Failed to get current path"))
        .expect("Failed to convert corrent path to absolute path");
    if path
        .read_dir()
        .expect("Failed to read files in parent dir")
        .into_iter()
        .any(|f| f.expect("sdlfk").file_name().eq("lib"))
        == false
    {
        println!("Failed to find lib directory in {:?}", path);
        exit(0);
    }
    println!("Write `exit` to close this program");
    let mut command = Command::new("flutter")
        .current_dir(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(["run"])
        .args(args)
        .spawn()
        .expect("Failed to run command");
    let command_stdin = Arc::new(Mutex::new(
        command.stdin.take().expect("Failed to get stdin"),
    ));
    let command_output = command
        .stdout
        .take()
        .expect("Failed to get command output stream");
    let command_err = command
        .stderr
        .take()
        .expect("Failed to get command error stream");

    let _ = print_output(command_output);
    let _ = print_err(command_err);
    let _ = watch_filesystem(command_stdin.clone(), Box::new(path.to_owned()));
    let t3 = proxy_user_messages(command_stdin.clone());

    let _ = t3.await;

    exit_flutter_app(command_stdin.clone());
    command.kill().expect("Failed to kill running process");

    exit(0);
}

fn print_output(command_output: ChildStdout) -> JoinHandle<()> {
    spawn(async {
        let reader = BufReader::new(command_output);
        reader
            .lines()
            .filter_map(|line| line.ok())
            .for_each(|line| println!("{}", line));
    })
}
fn print_err(command_output: ChildStderr) -> JoinHandle<()> {
    spawn(async {
        let reader = BufReader::new(command_output);
        reader
            .lines()
            .filter_map(|line| line.ok())
            .for_each(|line| println!("Error: {}", line));
    })
}

fn hot_reload(command_stdin: Arc<Mutex<ChildStdin>>) {
    sleep(Duration::from_millis(100)); // does not work without delay
    let stdin = command_stdin.clone();
    let mut stdin = stdin.lock().expect("Failed to lock stdin for writing");
    stdin.write(b"r").expect("Failed to write to stdin");
}

fn watch_filesystem(
    command_stdin: Arc<Mutex<ChildStdin>>,
    current_dir: Box<PathBuf>,
) -> JoinHandle<()> {
    spawn(async move {
        let (tx, rx) = mpsc::channel::<Result<Event>>();
        let mut watcher =
            notify::recommended_watcher(tx).expect("Failed to get recommended_watcher");
        let watcher_path =
            fs::canonicalize(current_dir.join("lib")).expect("Failed to locate lib directory");

        watcher
            .watch(watcher_path.as_path(), RecursiveMode::Recursive)
            .expect("Failed to watch current directory");
        for res in rx {
            match res {
                Ok(event) => match event.kind {
                    notify::EventKind::Modify(_) | notify::EventKind::Remove(_) => {
                        hot_reload(command_stdin.clone())
                    }
                    _ => println!("Unknown event: {:?}", event),
                },
                Err(e) => println!("Watch error: {:?}", e),
            }
        }
    })
}

fn proxy_user_messages(command_stdin: Arc<Mutex<ChildStdin>>) -> JoinHandle<()> {
    spawn(async move {
        let stdin = io::stdin();
        let mut user_input = String::new();
        while !user_input.contains("exit") {
            user_input.clear();
            stdin
                .read_line(&mut user_input)
                .expect("Failed to read user ");
            if user_input.contains("exit") {
                break;
            }
            let bytes = user_input.as_bytes();
            command_stdin
                .lock()
                .expect("Failed to aquired lock for stdin")
                .write(bytes)
                .expect("Failed to write to stdin");
        }
    })
}

fn exit_flutter_app(command_stdin: Arc<Mutex<ChildStdin>>) {
    command_stdin
        .lock()
        .expect("Failed to aquire lock for stdin")
        .write(b"q")
        .expect("Failed to write to stdin");
    sleep(Duration::from_secs(1));
}
