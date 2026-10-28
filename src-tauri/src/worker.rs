//! Locating and launching the Python extraction worker.
//!
//! The worker runs as a child process with a cleared environment, its own
//! working directory and `-B -E -s` so user site-packages, `PYTHON*`
//! variables and bytecode writes cannot influence it.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{
    error::{AppError, AppResult},
    models::ExtractorStatus,
};

/// Must match `researchpilot_worker.__version__` / `PROTOCOL_VERSION` / `RULESET_VERSION`.
pub const WORKER_VERSION: &str = "0.3.0";
pub const PROTOCOL_VERSION: i64 = 1;
pub const RULESET_VERSION: &str = "rules-v1";

// Interpreter and tool locations per platform. Only macOS is built and tested;
// Linux and Windows values are prepared for future ports.
#[cfg(target_os = "macos")]
const SEARCH_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";
#[cfg(target_os = "macos")]
const SYSTEM_PYTHONS: &[&str] = &[
    "/opt/homebrew/bin/python3",
    "/usr/local/bin/python3",
    "/usr/bin/python3",
];
#[cfg(all(unix, not(target_os = "macos")))]
const SEARCH_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
#[cfg(all(unix, not(target_os = "macos")))]
const SYSTEM_PYTHONS: &[&str] = &["/usr/bin/python3", "/usr/local/bin/python3"];
#[cfg(windows)]
const SYSTEM_PYTHONS: &[&str] = &[];

#[derive(Debug, Clone)]
pub struct WorkerEnvironment {
    /// Directory that contains the `researchpilot_worker` package.
    pub worker_root: PathBuf,
    pub requirements: PathBuf,
    /// Virtual environment managed by the app (`$APPDATA/extractor-env`).
    pub app_env_dir: PathBuf,
    /// Development-only interpreter (`python/.venv`), never used in release builds.
    pub dev_python: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PythonChoice {
    pub path: PathBuf,
    pub source: &'static str,
}

impl WorkerEnvironment {
    pub fn app_env_python(&self) -> PathBuf {
        venv_python(&self.app_env_dir)
    }

    pub fn worker_present(&self) -> bool {
        self.worker_root
            .join("researchpilot_worker")
            .join("__main__.py")
            .is_file()
    }

    /// Interpreter used for extraction: explicit override, app venv, dev venv, system.
    pub fn resolve_python(&self, override_path: &str) -> Option<PythonChoice> {
        let override_path = override_path.trim();
        if !override_path.is_empty() {
            let path = PathBuf::from(override_path);
            return path.is_file().then_some(PythonChoice {
                path,
                source: "override",
            });
        }
        let app_env = self.app_env_python();
        if app_env.is_file() {
            return Some(PythonChoice {
                path: app_env,
                source: "app_environment",
            });
        }
        if let Some(dev) = self.dev_python.as_ref().filter(|path| path.is_file()) {
            return Some(PythonChoice {
                path: dev.clone(),
                source: "development",
            });
        }
        system_python().map(|path| PythonChoice {
            path,
            source: "system",
        })
    }

    pub fn worker_command(&self, python: &Path, arguments: &[&str]) -> Command {
        let mut command = Command::new(python);
        command
            .args(["-B", "-E", "-s", "-m", "researchpilot_worker"])
            .args(arguments)
            .current_dir(&self.worker_root);
        isolate_environment(&mut command);
        command
    }

    pub fn status(&self, override_path: &str, grobid_url: Option<&str>) -> ExtractorStatus {
        let python = self.resolve_python(override_path);
        let worker_present = self.worker_present();
        let mut status = ExtractorStatus {
            python_path: python
                .as_ref()
                .map(|choice| choice.path.to_string_lossy().into_owned()),
            python_source: python.as_ref().map(|choice| choice.source.to_owned()),
            worker_root: self.worker_root.to_string_lossy().into_owned(),
            worker_present,
            app_environment_ready: self.app_env_python().is_file(),
            capabilities: None,
            message: None,
        };
        let Some(python) = python else {
            status.message = Some(if override_path.trim().is_empty() {
                "No se encontró Python 3. Instálalo (por ejemplo con Homebrew) para habilitar la extracción científica.".into()
            } else {
                "La ruta de Python configurada no existe.".into()
            });
            return status;
        };
        if !worker_present {
            status.message =
                Some("No se encontró el paquete del extractor junto a la aplicación.".into());
            return status;
        }
        let mut arguments = vec!["capabilities"];
        if let Some(url) = grobid_url {
            arguments.extend(["--grobid-url", url]);
        }
        match run_with_timeout(
            self.worker_command(&python.path, &arguments),
            Duration::from_secs(20),
        ) {
            Ok(output) if output.success => match serde_json::from_str(&output.stdout) {
                Ok(capabilities) => status.capabilities = Some(capabilities),
                Err(error) => {
                    status.message = Some(format!("Respuesta inválida del extractor: {error}"))
                }
            },
            Ok(output) => {
                status.message = Some(format!(
                    "El extractor no respondió correctamente: {}",
                    tail(&output.stderr, 300)
                ))
            }
            Err(error) => status.message = Some(error.to_string()),
        }
        status
    }
}

/// Interpreter inside a virtual environment created by `python -m venv`.
pub fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python3")
    }
}

pub fn system_python() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = SYSTEM_PYTHONS.iter().map(PathBuf::from).collect();
    if cfg!(windows) {
        candidates.extend(windows_pythons());
    }
    candidates.into_iter().find(|path| {
        if !path.is_file() {
            return false;
        }
        // macOS: /usr/bin/python3 is a shim that opens an installer dialog when the
        // Command Line Tools are missing; only use it when they are present.
        !cfg!(target_os = "macos")
            || path != Path::new("/usr/bin/python3")
            || Path::new("/Library/Developer/CommandLineTools/usr/bin/python3").is_file()
            || Path::new("/Applications/Xcode.app").is_dir()
    })
}

/// python.org installs per user: `%LOCALAPPDATA%\Programs\Python\Python3xx\python.exe`,
/// newest first.
fn windows_pythons() -> Vec<PathBuf> {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let root = PathBuf::from(local).join("Programs").join("Python");
    let mut found: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("Python3"))
        })
        .map(|path| path.join("python.exe"))
        .collect();
    found.sort();
    found.reverse();
    found
}

pub fn isolate_environment(command: &mut Command) {
    command
        .env_clear()
        .env("LANG", "en_US.UTF-8")
        .env("PYTHONIOENCODING", "utf-8");
    #[cfg(unix)]
    command.env("PATH", SEARCH_PATH);
    // Windows needs its system variables to start any process.
    let kept: &[&str] = if cfg!(windows) {
        &[
            "PATH",
            "SYSTEMROOT",
            "USERPROFILE",
            "TEMP",
            "TMP",
            "LOCALAPPDATA",
            "APPDATA",
        ]
    } else {
        &["HOME", "TMPDIR"]
    };
    for key in kept {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

pub fn is_loopback_url(url: &str) -> bool {
    let without_scheme = url
        .trim()
        .strip_prefix("http://")
        .or_else(|| url.trim().strip_prefix("https://"))
        .unwrap_or("");
    let authority = without_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Run a short command, killing it if it exceeds `timeout`.
pub fn run_with_timeout(mut command: Command, timeout: Duration) -> AppResult<CommandOutput> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| AppError::Worker(format!("No se pudo iniciar el proceso: {error}")))?;
    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::Worker(
                "El proceso excedió el tiempo máximo".into(),
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    Ok(CommandOutput {
        success: status.success(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

fn read_in_background<R: Read + Send + 'static>(source: Option<R>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut source) = source {
            let _ = source.read_to_string(&mut text);
        }
        text
    })
}

/// Keep the last `limit` characters, cut on a char boundary.
pub fn tail(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    trimmed.chars().skip(count.saturating_sub(limit)).collect()
}

pub fn kill_quietly(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_constants_match_the_python_worker() {
        let source = include_str!("../../python/researchpilot_worker/__init__.py");
        assert!(source.contains(&format!("__version__ = \"{WORKER_VERSION}\"")));
        assert!(source.contains(&format!("PROTOCOL_VERSION = {PROTOCOL_VERSION}")));
        assert!(source.contains(&format!("RULESET_VERSION = \"{RULESET_VERSION}\"")));
    }

    #[test]
    fn detects_loopback_urls_only() {
        assert!(is_loopback_url("http://127.0.0.1:8070"));
        assert!(is_loopback_url("http://localhost:8070/api"));
        assert!(is_loopback_url("http://[::1]:8070"));
        assert!(!is_loopback_url("https://grobid.example.org"));
        assert!(!is_loopback_url("http://10.0.0.5:8070"));
        assert!(!is_loopback_url("http://127.0.0.1.evil.example:8070"));
    }

    #[test]
    fn override_python_must_exist() {
        let environment = WorkerEnvironment {
            worker_root: PathBuf::from("/nonexistent"),
            requirements: PathBuf::from("/nonexistent/requirements.txt"),
            app_env_dir: PathBuf::from("/nonexistent/env"),
            dev_python: None,
        };
        assert_eq!(environment.resolve_python("/nonexistent/python3"), None);
    }
}
