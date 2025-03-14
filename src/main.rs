use clap::Parser;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::io::{self, Write};
use reqwest;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::path::Path;
use url::Url;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use once_cell::sync::Lazy;

static URLS: Lazy<Mutex<Urls>> = Lazy::new(|| {
    Mutex::new(Urls {
        x86_64_cmake: String::from("https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6-windows-x86_64.zip"),
        i386_cmake: String::from("https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6-windows-i386.zip"),
        x86_64_gcc: String::from("https://github.com/niXman/mingw-builds-binaries/releases/download/14.1.0-rt_v12-rev0/x86_64-14.1.0-release-posix-seh-ucrt-rt_v12-rev0.7z"),
        i686_gcc: String::from("https://github.com/niXman/mingw-builds-binaries/releases/download/14.1.0-rt_v12-rev0/i686-14.1.0-release-posix-dwarf-ucrt-rt_v12-rev0.7z"),
    })
});

#[derive(Serialize, Deserialize, Debug)]
struct Urls {
    x86_64_cmake: String,
    i386_cmake: String,
    x86_64_gcc: String,
    i686_gcc: String,
}

#[derive(Parser, Debug)]
#[command(version, about = "CMake Build System by Rust", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "x64", help = "x64/x86")]
    architecture: String,
    #[arg(short, long, default_value = "debug", help = "debug/release")]
    build_type: String,
    #[arg(short, long, default_value = "static", help = "static/shared")]
    library_type: String,
    #[arg(short, long, default_value_t = get_current_dir_name(), help = "program name to run")]
    program_name: String,
}

fn get_filename(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.path_segments().map(|s| s.last().unwrap().to_string()))
        .unwrap_or_else(|| "download.bin".to_string())
}

fn get_current_dir_name() -> String {
    env::current_dir()
        .ok()
        .and_then(|p: PathBuf| {
            p.file_name()
             .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "unknown".into())
}

fn is_program_in_path(program: &str) -> bool {
    Command::new(program)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

async fn if_download(name: &str) {
    print!("是否现在下载{}？(y/n): ", name);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("无法读取输入");
    let input = input.trim().to_lowercase();

    match input.as_str() {
        "y" => {
            let urls = URLS.lock().unwrap();
            let url = match name {
                "x86_64_cmake" => &urls.x86_64_cmake,
                "i386_cmake" => &urls.i386_cmake,
                "x86_64_gcc" => &urls.x86_64_gcc,
                "i686_gcc" => &urls.i686_gcc,
                _ => {
                    eprintln!("未知组件: {}", name);
                    return;
                }
            };
            match download(url).await {
                Ok(path) => println!("文件已保存至: {}", path),
                Err(e) => eprintln!("下载失败: {}", e),
            }
        }
        "n" => {
            std::process::exit(1);
        }
        _ => println!("无效输入，请输入 'y' 或 'n'"),
    }
}

async fn download(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let save_dir = "downloads";
    let show_progress = true;

    std::fs::create_dir_all(save_dir)?;

    let filename = get_filename(url);
    let save_path = Path::new(save_dir).join(&filename);

    let client = reqwest::Client::new();
    let response = client.head(url).send().await?;
    let total_size = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|ct_len| ct_len.to_str().ok())
        .and_then(|ct_len| ct_len.parse::<u64>().ok())
        .unwrap_or(0);

    let pb = if show_progress {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };

    let mut response = reqwest::get(url).await?;
    let mut file = File::create(&save_path)?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if let Some(ref pb) = pb {
            pb.set_position(downloaded);
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("下载完成");
    }

    Ok(save_path.to_string_lossy().to_string())
}

async fn envrionment_check() {
    if !is_program_in_path("cmake") {
        println!("CMake is not installed or not in PATH");
        let arch = std::env::consts::ARCH;

        match arch {
            "x86_64" => if_download("x86_64_cmake").await,
            "x86" => if_download("i386_cmake").await,
            _ => {
                println!("Unknown architecture: {}", arch);
                std::process::exit(1);
            }
        }
    }

    if !is_program_in_path("x86_64-w64-mingw32-gcc") {
        println!("x86_64 GCC is not installed or not in PATH");
        if_download("x86_64_gcc").await;
    }

    if !is_program_in_path("i686-w64-mingw32-gcc") {
        println!("i686 GCC is not installed or not in PATH");
        if_download("i686_gcc").await;
    }
}

#[tokio::main]
async fn main() {
    let _args = Args::parse();
    envrionment_check().await;
}