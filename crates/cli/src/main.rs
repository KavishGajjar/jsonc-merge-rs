use clap::{Args, Parser, Subcommand};
use jsonc_merge::{Document, Limits, MergePlan, Policy, merge};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    name = "jsonc-merge",
    version,
    about = "Merge JSONC configurations using explicit record identities"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate JSONC syntax and every declared identity policy.
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    /// Print a schema-versioned JSON report (exit 1 if conflicted).
    Plan(Inputs),
    /// Print a clean JSONC merge; on conflict stdout stays empty.
    Merge(Inputs),
    /// Git merge driver: replace OURS with a clean result or whole-file diff3 conflict.
    GitDriver {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(default_value = "7", value_parser = clap::value_parser!(u16).range(1..=80))]
        marker_width: u16,
    },
}

#[derive(Args)]
struct Inputs {
    base: PathBuf,
    ours: PathBuf,
    theirs: PathBuf,
    #[arg(long)]
    policy: Option<PathBuf>,
}

struct AppError {
    code: &'static str,
    message: String,
    span: Option<jsonc_merge::Span>,
    input: Option<jsonc_merge::Side>,
    file: Option<PathBuf>,
}
impl AppError {
    fn at_file(mut self, path: &Path) -> Self {
        self.file = Some(path.to_owned());
        self
    }
}
impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        Self {
            code: "io",
            message: e.to_string(),
            span: None,
            input: None,
            file: None,
        }
    }
}
impl From<jsonc_merge::Diagnostic> for AppError {
    fn from(e: jsonc_merge::Diagnostic) -> Self {
        Self {
            code: match e.code {
                jsonc_merge::DiagnosticCode::Syntax => "syntax",
                jsonc_merge::DiagnosticCode::DuplicateKey => "duplicate_key",
                jsonc_merge::DiagnosticCode::Policy => "policy",
                jsonc_merge::DiagnosticCode::Identity => "identity",
                jsonc_merge::DiagnosticCode::Budget => "budget",
                jsonc_merge::DiagnosticCode::Conflict => "conflict",
                jsonc_merge::DiagnosticCode::StalePlan => "stale_plan",
                jsonc_merge::DiagnosticCode::Render => "render",
            },
            message: e.to_string(),
            span: e.span,
            input: e.input,
            file: None,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let _ = writeln!(
                io::stderr().lock(),
                "{}",
                serde_json::json!({"error":{"code":error.code,"message":error.message,"span":error.span,"input":error.input,"file":error.file.as_ref().map(|path| path.to_string_lossy())}})
            );
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<u8, AppError> {
    match cli.command {
        Command::Check { files, policy } => {
            let policy = load_policy(policy.as_deref())?;
            for path in files {
                policy
                    .validate(&read_document(&path)?)
                    .map_err(|e| AppError::from(e).at_file(&path))?;
            }
            Ok(0)
        }
        Command::Plan(inputs) => {
            let plan = load_plan(&inputs)?;
            let report = serde_json::to_string_pretty(&plan.report())
                .expect("report serialization cannot fail");
            writeln!(io::stdout().lock(), "{report}")?;
            Ok(if plan.is_clean() { 0 } else { 1 })
        }
        Command::Merge(inputs) => {
            let plan = load_plan(&inputs)?;
            if !plan.is_clean() {
                print_conflicts(&plan)?;
                return Ok(1);
            }
            let result = plan.render()?;
            io::stdout().lock().write_all(result.as_bytes())?;
            Ok(0)
        }
        Command::GitDriver {
            inputs,
            marker_width,
        } => git_driver(inputs, marker_width),
    }
}

fn read(path: &Path) -> Result<String, AppError> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| AppError::from(e).at_file(path))?
        .take((Limits::default().max_input_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| AppError::from(e).at_file(path))?;
    if bytes.len() > Limits::default().max_input_bytes {
        return Err(AppError {
            code: "budget",
            message: format!("{} exceeds the input byte limit", path.display()),
            span: None,
            input: None,
            file: Some(path.to_owned()),
        });
    }
    String::from_utf8(bytes).map_err(|e| AppError {
        code: "utf8",
        message: format!("{}: {e}", path.display()),
        span: None,
        input: None,
        file: Some(path.to_owned()),
    })
}

fn load_policy(path: Option<&Path>) -> Result<Policy, AppError> {
    path.map(|p| Policy::from_json(&read(p)?).map_err(|e| AppError::from(e).at_file(p)))
        .unwrap_or_else(|| Ok(Policy::default()))
}
fn read_document(path: &Path) -> Result<Document, AppError> {
    Document::parse(read(path)?).map_err(|e| AppError::from(e).at_file(path))
}
fn load_plan(inputs: &Inputs) -> Result<MergePlan, AppError> {
    let policy = load_policy(inputs.policy.as_deref())?;
    let base = read_document(&inputs.base)?;
    let ours = read_document(&inputs.ours)?;
    let theirs = read_document(&inputs.theirs)?;
    merge(&base, &ours, &theirs, &policy).map_err(|e| {
        let path = match e.input {
            Some(jsonc_merge::Side::Base) => Some(&inputs.base),
            Some(jsonc_merge::Side::Ours) => Some(&inputs.ours),
            Some(jsonc_merge::Side::Theirs) => Some(&inputs.theirs),
            None => None,
        };
        let mut error = AppError::from(e);
        error.file = path.cloned();
        error
    })
}
fn print_conflicts(plan: &MergePlan) -> Result<(), AppError> {
    writeln!(
        io::stderr().lock(),
        "{}",
        serde_json::to_string(&plan.report()).expect("report serialization cannot fail")
    )?;
    Ok(())
}

fn git_driver(inputs: Inputs, width: u16) -> Result<u8, AppError> {
    for path in [&inputs.base, &inputs.ours, &inputs.theirs] {
        if !fs::symlink_metadata(path)
            .map_err(|e| AppError::from(e).at_file(path))?
            .file_type()
            .is_file()
        {
            return Err(AppError {
                code: "git_input",
                message: format!(
                    "Git input must be a regular file, not a symlink: {}",
                    path.display()
                ),
                span: None,
                input: None,
                file: Some(path.to_owned()),
            });
        }
    }
    let plan = load_plan(&inputs)?;
    let output = if plan.is_clean() {
        plan.render()?
    } else {
        let b = read(&inputs.base)?;
        let o = read(&inputs.ours)?;
        let t = read(&inputs.theirs)?;
        let mut output = String::new();
        let mut section = |marker: char, label: &str, text: &str| {
            output.extend(std::iter::repeat_n(marker, usize::from(width)));
            output.push_str(label);
            output.push('\n');
            output.push_str(text);
            if !text.ends_with('\n') {
                output.push('\n');
            }
        };
        section('<', " OURS", &o);
        section('|', " BASE", &b);
        section('=', "", &t);
        output.extend(std::iter::repeat_n('>', usize::from(width)));
        output.push_str(" THEIRS\n");
        print_conflicts(&plan)?;
        output
    };
    replace_scratch(&inputs.ours, output.as_bytes()).map_err(|e| e.at_file(&inputs.ours))?;
    Ok(if plan.is_clean() { 0 } else { 1 })
}

fn replace_scratch(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.as_file()
        .set_permissions(fs::metadata(path)?.permissions())?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| AppError::from(e.error))?;
    Ok(())
}
