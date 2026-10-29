use std::{
    env,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{self, Command},
};

struct Config {
    workspace_root: PathBuf,
    backend_dir: PathBuf,
    frontend_dir: PathBuf,
}

const USAGE: &str = "Usage: cargo xtask <task> [args…]\n\nTasks:\n  dev           Run the backend + frontend together against the dev database\n  test-server   Run the backend against a fresh throwaway test database (see backend/.env.test)\n  test-reset    Delete the test database so the next test run starts clean\n  e2e           Run the Playwright end-to-end suite (see e2e/); extra args pass through\n  install       Install frontend dependencies (yarn install)\n  export-fixtures [--out DIR]\n                Write golden engine fixtures for the macOS app's parity tests";

fn main() {
    let mut args = env::args().skip(1);
    let task = args.next().unwrap_or_else(|| {
        eprintln!("{USAGE}");
        process::exit(1);
    });
    let cfg = config_from_metadata();

    match task.as_str() {
        "dev" => run_dev(&cfg),
        "test-server" => run_test_server(&cfg),
        "test-reset" => reset_test_db(&cfg),
        "e2e" => run_e2e(&cfg, args.collect()),
        "install" => ensure_frontend_deps(&cfg.frontend_dir),
        "export-fixtures" => export_fixtures(&cfg, args.collect()),
        _ => {
            eprintln!("Unknown task: {task}\n\n{USAGE}");
            process::exit(1);
        }
    }
}

fn config_from_metadata() -> Config {
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .expect("cargo metadata failed");
    let root = metadata.workspace_root.as_std_path().to_path_buf();

    Config {
        backend_dir: root.join("backend"),
        frontend_dir: root.join("frontend"),
        workspace_root: root,
    }
}

/// Run the backend's ignored `fixture_export` test, which replays fixed
/// scenarios through the real engines and writes `{inputs, expected}` JSON
/// for the native macOS app's parity tests. `--out DIR` picks the output
/// directory (default: `target/parity-fixtures`).
fn export_fixtures(cfg: &Config, args: Vec<String>) {
    let out = match args.as_slice() {
        [] => cfg.workspace_root.join("target").join("parity-fixtures"),
        [flag, dir] if flag == "--out" => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                env::current_dir().expect("current dir").join(dir)
            }
        }
        _ => {
            eprintln!("Usage: cargo xtask export-fixtures [--out DIR]");
            process::exit(1);
        }
    };
    let status = Command::new("cargo")
        .args([
            "test",
            "-p",
            "lifetime_income_planner",
            "fixture_export::export_parity_fixtures",
            "--",
            "--ignored",
            "--exact",
            "--nocapture",
        ])
        .env("FIXTURE_OUT_DIR", &out)
        .current_dir(&cfg.workspace_root)
        .status()
        .unwrap_or_else(|e| panic!("failed to run cargo test: {e}"));
    process::exit(status.code().unwrap_or(1));
}

/// Install frontend dependencies if `node_modules` isn't present yet, so a fresh
/// checkout can run `cargo xtask dev` without a separate setup step.
fn ensure_frontend_deps(frontend_dir: &Path) {
    if frontend_dir.join("node_modules").is_dir() {
        return;
    }
    println!("Installing frontend dependencies (yarn install)…");
    run("yarn", &["install"], frontend_dir);
}

/// Run the Playwright end-to-end suite in `e2e/`. The suite's own global setup
/// launches an isolated test backend (port 8091) + frontend and tears them down
/// afterwards, so this just ensures the e2e deps are installed and hands off to
/// the runner script. Extra args pass straight through, e.g.
/// `cargo xtask e2e tests/auth/login.spec.ts` or `cargo xtask e2e --report`.
fn run_e2e(cfg: &Config, extra_args: Vec<String>) {
    let e2e_dir = cfg.workspace_root.join("e2e");
    ensure_e2e_deps(&e2e_dir);

    let script = e2e_dir.join("scripts").join("run-tests.sh");
    let status = Command::new("bash")
        .arg(&script)
        .args(&extra_args)
        .current_dir(&e2e_dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to run e2e suite: {e}"));
    process::exit(status.code().unwrap_or(1));
}

/// Install the e2e Playwright dependencies if they're missing so `cargo xtask
/// e2e` works on a fresh checkout. Browser binaries still need a one-time
/// `npx playwright install chromium` (documented in e2e/README.md).
fn ensure_e2e_deps(e2e_dir: &Path) {
    if e2e_dir.join("node_modules").is_dir() {
        return;
    }
    println!("Installing e2e dependencies (npm install)…");
    run("npm", &["install"], e2e_dir);
}

/// Run the Actix backend and the Vite frontend together. When either process
/// exits, the other is killed so the two never outlive each other.
fn run_dev(cfg: &Config) {
    // Prereq: make sure the frontend can start.
    ensure_frontend_deps(&cfg.frontend_dir);

    // `.process_group(0)` puts each child in its own process group so we can
    // later signal the whole tree — the wrappers (`cargo run`, `yarn`) each
    // spawn the real long-running process (the server binary, `vite`) as a
    // grandchild, and a group SIGTERM reaches those too.
    println!("Starting backend  → http://127.0.0.1:8080  (docs at /docs/)");
    let mut backend = Command::new("cargo")
        .args(["run", "--package", "lifetime_income_planner"])
        .current_dir(&cfg.backend_dir)
        .process_group(0)
        .spawn()
        .expect("failed to spawn backend (cargo run)");

    println!("Starting frontend → http://localhost:5173");
    let mut frontend = Command::new("yarn")
        .args(["dev"])
        .current_dir(&cfg.frontend_dir)
        .process_group(0)
        .spawn()
        .unwrap_or_else(|e| {
            kill_group(backend.id());
            panic!("failed to spawn frontend (yarn dev): {e}");
        });

    // Silence the unused-field warning while keeping the field for future tasks.
    let _ = &cfg.workspace_root;

    // Because the children are in their own process groups, a terminal Ctrl-C no
    // longer reaches them directly — so this handler is what tears them down on
    // Ctrl-C / SIGTERM. The try_wait loop below covers a child exiting on its own.
    let backend_id = backend.id();
    let frontend_id = frontend.id();
    ctrlc::set_handler(move || {
        eprintln!("\nStopping…");
        kill_group(backend_id);
        kill_group(frontend_id);
        process::exit(130);
    })
    .expect("failed to install signal handler");

    loop {
        if let Some(status) = frontend.try_wait().expect("failed to wait on frontend") {
            kill_group(backend.id());
            process::exit(status.code().unwrap_or(1));
        }
        if let Some(status) = backend.try_wait().expect("failed to wait on backend") {
            kill_group(frontend.id());
            process::exit(status.code().unwrap_or(1));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// Run the backend against the throwaway test database defined in
/// `backend/.env.test`, recreating it fresh so every run starts clean. This
/// keeps automated/manual API testing fully isolated from the dev database in
/// `lifetime_income_planner.db`.
fn run_test_server(cfg: &Config) {
    let test_env = load_env_file(&cfg.backend_dir.join(".env.test"));
    guard_test_db(cfg, &test_env);
    reset_test_db(cfg);

    let port = get_env(&test_env, "PORT").unwrap_or_else(|| "8091".to_string());
    println!("Starting TEST backend → http://127.0.0.1:{port}  (fresh test database)");

    let mut backend = Command::new("cargo")
        .args(["run", "--package", "lifetime_income_planner"])
        .current_dir(&cfg.backend_dir)
        .envs(test_env)
        .process_group(0)
        .spawn()
        .expect("failed to spawn test backend (cargo run)");

    // Children run in their own process group, so terminal Ctrl-C won't reach
    // them directly — tear the group down explicitly on Ctrl-C / SIGTERM.
    let backend_id = backend.id();
    ctrlc::set_handler(move || {
        eprintln!("\nStopping…");
        kill_group(backend_id);
        process::exit(130);
    })
    .expect("failed to install signal handler");

    let status = backend.wait().expect("failed to wait on test backend");
    process::exit(status.code().unwrap_or(0));
}

/// Delete the test database (and its SQLite sidecar files). Only ever touches
/// the path from `.env.test`, never the dev database.
fn reset_test_db(cfg: &Config) {
    let test_env = load_env_file(&cfg.backend_dir.join(".env.test"));
    guard_test_db(cfg, &test_env);
    let db = get_env(&test_env, "DATABASE_URL").expect("DATABASE_URL missing from .env.test");
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let path = cfg.backend_dir.join(format!("{db}{suffix}"));
        if path.exists() {
            std::fs::remove_file(&path).unwrap_or_else(|e| {
                panic!("failed to remove {}: {e}", path.display());
            });
        }
    }
}

/// Refuse to run if the test database somehow points at the dev database, so a
/// misconfiguration can never wipe the user's manual data.
fn guard_test_db(cfg: &Config, test_env: &[(String, String)]) {
    let test_db = get_env(test_env, "DATABASE_URL").expect("DATABASE_URL missing from .env.test");
    let dev_db = get_env(&load_env_file(&cfg.backend_dir.join(".env")), "DATABASE_URL")
        .unwrap_or_default();
    if test_db == dev_db {
        eprintln!(
            "Refusing to run: .env.test DATABASE_URL ({test_db}) is the same as the dev database. \
             Point .env.test at a separate file."
        );
        process::exit(1);
    }
}

/// Minimal `.env` parser: `KEY=VALUE` per line, ignoring blanks and `#` comments.
fn load_env_file(path: &Path) -> Vec<(String, String)> {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    contents
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn get_env(env: &[(String, String)], key: &str) -> Option<String> {
    env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// Send SIGTERM to the entire process group led by `pid`. A child spawned with
/// `.process_group(0)` has a group id equal to its pid, so this terminates that
/// child and every process it started (e.g. `cargo run`'s server binary, or
/// `yarn`'s `vite`). Best-effort: a group that has already exited is ignored.
fn kill_group(pid: u32) {
    // Safety: `kill` is always safe to call; an invalid/dead pgid just returns
    // an error, which we deliberately ignore.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
}

fn run(cmd: &str, args: &[&str], dir: &Path) {
    let status = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn `{cmd}`: {e}"));
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
}
