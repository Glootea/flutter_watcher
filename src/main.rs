use notify::{Event, RecursiveMode, Result, Watcher};
use std::{
    io::{self, BufRead, BufReader, Write},
    path::Path,
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
    let args = std::env::args().into_iter().skip(1);
    println!("Write `exit` to close this program");
    let mut command = Command::new("flutter")
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
    let command_output = command.stdout.take().unwrap();
    let command_err = command.stderr.take().unwrap();

    let _ = print_output(command_output);
    let _ = print_err(command_err);
    let _ = watch_filesystem(command_stdin.clone());
    let t3 = proxy_user_messages(command_stdin.clone());

    let _ = t3.await;

    exit_flutter_app(command_stdin.clone());
    command.kill().unwrap();

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

fn watch_filesystem(command_stdin: Arc<Mutex<ChildStdin>>) -> JoinHandle<()> {
    spawn(async move {
        let (tx, rx) = mpsc::channel::<Result<Event>>();
        let mut watcher =
            notify::recommended_watcher(tx).expect("Failed to get recommended_watcher");
        watcher
            .watch(Path::new("./lib"), RecursiveMode::Recursive)
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
                .unwrap()
                .write(bytes)
                .expect("Failed to write to stdin");
        }
    })
}

fn exit_flutter_app(command_stdin: Arc<Mutex<ChildStdin>>) {
    command_stdin
        .lock()
        .unwrap()
        .write(b"q")
        .expect("Failed to write to stdin");
    sleep(Duration::from_secs(1));
}
