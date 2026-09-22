use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use luv::builder;
use luv::packager::{self, IconReport};
use luv::plugins::{self, Built};
use luv::project::{self, GameInfo, Project};
use luv::runtime::{Engine, EngineBuilder, Runtime, THREAD_STACK_SIZE};
use luv::vfs::{Pak, Vfs};
use luv::window::{DesktopWindows, WindowSystem};

#[derive(Parser)]
#[command(name = "luv", version, about = "The luv Luau game engine")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Create a new workspace")]
    Init {
        #[arg(default_value = ".", help = "Directory to create the workspace in")]
        path: PathBuf,
        #[arg(long, help = "Name of the game, defaults to the directory name")]
        name: Option<String>,
    },
    #[command(about = "Run the workspace straight from its source files")]
    Test {
        #[arg(default_value = ".", help = "Workspace directory")]
        path: PathBuf,
        #[arg(last = true, help = "Arguments passed to the game as Process.args")]
        args: Vec<String>,
    },
    #[command(about = "Compile the workspace into a bytecode package")]
    Build {
        #[arg(default_value = ".", help = "Workspace directory")]
        path: PathBuf,
    },
    #[command(about = "Run a built package")]
    Run {
        #[arg(default_value = ".", help = "A .luvit package, or a workspace directory to run its build output")]
        path: PathBuf,
        #[arg(last = true, help = "Arguments passed to the game as Process.args")]
        args: Vec<String>,
    },
    #[command(about = "Package the workspace into an executable that can be shipped")]
    Package {
        #[arg(default_value = ".", help = "Workspace directory")]
        path: PathBuf,
        #[arg(long, help = "Keep a console window open next to the game on Windows")]
        console: bool,
    },
}

fn main() -> ExitCode {
    let embedded = packager::embedded();
    let command = match &embedded {
        Some(executable) => Command::Run {
            path: executable.clone(),
            args: std::env::args().skip(1).collect(),
        },
        None => Cli::parse().command,
    };
    let packaged = embedded.is_some();
    let event_loop = match command {
        Command::Test { .. } | Command::Run { .. } => DesktopWindows::event_loop().ok(),
        _ => None,
    };
    let desktop = event_loop.as_ref().map(DesktopWindows::new);

    let engine = {
        let desktop = desktop.clone();
        thread::Builder::new()
            .name("luv".to_owned())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || {
                let result = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .map_err(anyhow::Error::from)
                    .and_then(|runtime| runtime.block_on(execute_command(command, desktop.clone(), packaged)));
                if let Some(desktop) = &desktop {
                    desktop.shutdown();
                }
                result
            })
    };

    let result = match engine {
        Ok(engine) => {
            if let Some(event_loop) = event_loop
                && let Err(error) = DesktopWindows::run(event_loop)
            {
                eprintln!("error: the window system stopped: {error}");
            }
            engine.join().unwrap_or_else(|_| Err(anyhow!("the engine crashed")))
        }
        Err(error) => Err(error.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            if let Some(executable) = &embedded {
                let title = executable
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Game".to_owned());
                let mut message = packager::remembered().join("\n\n");
                if !message.is_empty() {
                    message.push_str("\n\n");
                }
                message.push_str(&format!("{err:#}"));
                packager::alert(&title, &message);
            }
            ExitCode::FAILURE
        }
    }
}

async fn execute_command(command: Command, desktop: Option<DesktopWindows>, packaged: bool) -> Result<()> {
    let windows = desktop.map(|desktop| Arc::new(desktop) as Arc<dyn WindowSystem>);
    match command {
        Command::Init { path, name } => init(&path, name),
        Command::Test { path, args } => test(&path, args, windows).await,
        Command::Build { path } => build(&path).await,
        Command::Run { path, args } => run(&path, args, windows, packaged).await,
        Command::Package { path, console } => package(&path, console).await,
    }
}

async fn natives(project: &Project) -> Result<Vec<Built>> {
    let project = project.clone();
    tokio::task::spawn_blocking(move || plugins::build(&project)).await?
}

fn warn_unpacked(libraries: &[String]) {
    for library in libraries {
        eprintln!(
            "warning: {library} is a native library, so it is not packed into the game, move it into {}/ to ship it next to the game",
            plugins::NATIVE_DIR
        );
    }
}

fn init(path: &Path, name: Option<String>) -> Result<()> {
    let report = project::init(path, name)?;
    let root = report.root.display();
    if !report.existing {
        println!("Created workspace at {root}");
    } else if report.created.is_empty() && report.updated.is_empty() {
        println!("Workspace at {root} is already up to date");
    } else {
        println!("Updated workspace at {root}");
    }
    for file in &report.created {
        println!("  + {file}");
    }
    for file in &report.updated {
        println!("  ~ {file} (updated)");
    }
    for file in &report.skipped {
        println!("  = {file} (already exists)");
    }
    Ok(())
}

async fn test(path: &Path, args: Vec<String>, windows: Option<Arc<dyn WindowSystem>>) -> Result<()> {
    let project = Project::discover(path)?;
    let entry = project.entry()?;
    let vfs = project.source_vfs();
    if !vfs.is_file(&entry) {
        bail!("main script `{entry}` does not exist in {}", project.root.display());
    }
    let libraries = natives(&project).await?;
    for library in &libraries {
        if library.rebuilt {
            eprintln!("Built native library {}", library.file);
        }
    }
    for container in builder::build_containers(&project, &libraries).await? {
        if container.rebuilt {
            eprintln!("Built container {} v{}", container.name, container.version);
        }
    }
    let game = Engine::builder(Arc::new(vfs))
        .args(args)
        .game(project.manifest.game.name.clone(), project.root.clone())
        .icon(project.manifest.game.icon.as_deref())
        .library_dirs([project.output_dir()])
        .container_dirs([project.output_dir()]);
    execute(game, &entry, windows, false).await
}

async fn build(path: &Path) -> Result<()> {
    let project = Project::discover(path)?;
    let game = &project.manifest.game;
    let report = builder::build(&project).await?;
    let libraries = natives(&project).await?;
    let containers = builder::build_containers(&project, &libraries).await?;
    println!(
        "Built {} v{} ({}, {})",
        game.name,
        game.version,
        count(report.scripts, "script"),
        count(report.assets, "asset")
    );
    println!(
        "  {} -> {} at {}",
        format_size(report.raw_bytes),
        format_size(report.package_bytes),
        report.package.display()
    );
    for library in &libraries {
        println!("  + {} (native library)", library.file);
    }
    for container in &containers {
        println!(
            "  + {} (container {} v{})",
            container_file(&container.path),
            container.name,
            container.version
        );
    }
    warn_unpacked(&report.unpacked_libraries);
    Ok(())
}

fn container_file(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

async fn package(path: &Path, console: bool) -> Result<()> {
    let project = Project::discover(path)?;
    let report = builder::build(&project).await?;
    let libraries = natives(&project).await?;
    let containers: Vec<PathBuf> = builder::build_containers(&project, &libraries)
        .await?
        .into_iter()
        .map(|container| container.path)
        .collect();
    let workspace = project.clone();
    let game = report.package.clone();
    let packaged = tokio::task::spawn_blocking(move || {
        packager::package(&workspace, &game, &libraries, &containers, console)
    })
    .await??;
    let info = &project.manifest.game;
    println!(
        "Packaged {} v{} into {} ({})",
        info.name,
        info.version,
        packaged.executable.display(),
        format_size(packaged.bytes)
    );
    match &packaged.icon {
        IconReport::Embedded(source) => println!("  + icon from {source}"),
        IconReport::Beside { source, file } => println!("  + {file} (icon from {source})"),
        IconReport::Missing(_) | IconReport::Unset => {}
    }
    for library in &packaged.libraries {
        println!("  + {library}");
    }
    for container in &packaged.containers {
        println!("  + {container}");
    }
    if let IconReport::Missing(source) = &packaged.icon {
        eprintln!("warning: the icon {source} set in build.toml does not exist, so the game has no icon");
    }
    warn_unpacked(&report.unpacked_libraries);
    if cfg!(debug_assertions) {
        println!("note: this luv is a debug build, package with a release build of luv for the best speed");
    }
    Ok(())
}

async fn run(path: &Path, args: Vec<String>, windows: Option<Arc<dyn WindowSystem>>, packaged: bool) -> Result<()> {
    let package = if path.is_dir() {
        Project::discover(path)?.package_path()
    } else {
        path.to_path_buf()
    };
    let pak = Pak::open(&package).with_context(|| format!("failed to open {}", package.display()))?;
    let info = GameInfo::from_manifest(pak.manifest())?;
    let directory = package.parent().map(Path::to_path_buf).unwrap_or_default();
    let game = Engine::builder(Arc::new(pak))
        .args(args)
        .game(info.name.clone(), directory.clone())
        .icon(info.icon.as_deref())
        .container_dirs([directory]);
    execute(game, &info.main, windows, packaged).await
}

async fn execute(
    game: EngineBuilder,
    entry: &str,
    windows: Option<Arc<dyn WindowSystem>>,
    packaged: bool,
) -> Result<()> {
    let game = if packaged {
        game.reporter(|message| {
            eprintln!("error: {message}");
            packager::remember(message);
        })
    } else {
        game
    };
    let engine = match windows {
        Some(windows) => game.windows(windows).build(),
        None => game.build(),
    };
    let interrupted = engine.clone();
    let interrupt = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupted.request_exit(130);
            if tokio::signal::ctrl_c().await.is_ok() {
                std::process::exit(130);
            }
        }
    });
    Runtime::new(engine.clone())?.run(entry).await;
    interrupt.abort();
    if let Some(code) = engine.exit_code() {
        let _ = std::io::stdout().flush();
        std::process::exit(code);
    }
    match engine.errors() {
        0 => Ok(()),
        1 => bail!("the game stopped after 1 uncaught error"),
        errors => bail!("the game stopped after {errors} uncaught errors"),
    }
}

fn count(amount: usize, noun: &str) -> String {
    if amount == 1 { format!("1 {noun}") } else { format!("{amount} {noun}s") }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
