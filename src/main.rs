use clap::Parser;
use lazy_static::lazy_static;
use std::{
    env,
    io::ErrorKind,
    process::{Command, ExitStatus},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser, Debug)]
#[command(version, about = "CMake Build System in Rust", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "vscode")]
    config_ide: String,
    #[arg(short, long, default_value = "x64")]
    architecture: String,
    #[arg(short, long, default_value = "Debug")]
    build_type: String,
    #[arg(short, long, default_value = get_current_dir_name())]
    program_name: Option<String>,
}

lazy_static! {
    static ref CURRENT_DIR_NAME: String = {
        let current_dir = env::current_dir()
            .expect("Failed to get current directory");

        current_dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                eprintln!("Warning: Current directory is root, using default name 'root'");
                "root".to_string()
            })
    };
}

fn main() {
    if let Err(e) = run() {
        eprintln!("❌ Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let arch = args.architecture;
    let build_type = args.build_type;
    let program_name = args.program_name.unwrap_or_else(|| get_current_dir_name().to_string());
    let build_dir = format!("build/{}-{}", build_type, arch);
    let bin_dir = env::current_dir()?.join(&build_dir).join("bin").to_str().unwrap().to_string();

    println!("{}", bin_dir);

    let (generator, flags, c_compiler, cxx_compiler) = match std::env::consts::OS {
        "windows" => configure_windows(&arch),
        "linux" => configure_linux(&arch),
        os => return Err(format!("Unsupported OS: {}", os).into()),
    };

    let config_params = [
        "-B", &build_dir,
        "-S", ".",
        "-G", generator,
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
        &format!("-DCMAKE_BUILD_TYPE={}", build_type),
        &format!("-DCMAKE_C_FLAGS={}", flags),
        &format!("-DCMAKE_CXX_FLAGS={}", flags),
        &format!("-DEXECUTABLE_OUTPUT_PATH={}", bin_dir),
        &format!("-DLIBRARY_OUTPUT_PATH={}", bin_dir),
        &format!("-DCMAKE_C_COMPILER={}", c_compiler),
        &format!("-DCMAKE_CXX_COMPILER={}", cxx_compiler),
    ];

    run_command("cmake", &config_params)?;

    let build_params = [
        "--build",
        &build_dir,
    ];

    run_command("cmake", &build_params)?;

    let exe_path = format!("{}/{}", bin_dir, program_name);
    run_command(&exe_path, &[])?;

    Ok(())
}

fn configure_windows(arch: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match arch {
        "x64" => ("MinGW Makefiles", "-m64", "x86_64-w64-mingw32-gcc.exe", "x86_64-w64-mingw32-g++.exe"),
        "x86" => ("MinGW Makefiles", "-m32", "i686-w64-mingw32-gcc.exe", "i686-w64-mingw32-g++.exe"),
        _ => ("", "", "", ""),
    }
}

fn configure_linux(arch: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match arch {
        "x64" => ("Unix Makefiles", "-m64", "gcc", "g++"),
        "x86" => ("Unix Makefiles", "-m32", "gcc", "g++"),
        _ => ("", "", "", ""),
    }
}

fn get_current_dir_name() -> &'static str {
    &CURRENT_DIR_NAME
}

fn run_command(command: &str, args: &[&str]) -> Result<ExitStatus> {
    println!("🚀 Executing: {} {}", command, args.join(" "));

    let status = Command::new(command)
        .args(args)
        .status()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                format!("Command not found: {}", command)
            } else {
                format!("Command failed: {}", e)
            }
        })?;

    if status.success() {
        Ok(status)
    } else {
        Err(format!("Command execution failed with status: {}", status).into())
    }
}