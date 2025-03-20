use clap::{Parser};
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use sevenz_rust;
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{self, BufWriter, ErrorKind, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};
use url::Url;
use zip::ZipArchive;

// 常量定义
const DOWNLOAD_DIR: &str = "downloads";
const TOOLS_DIR: &str = "tools";
const CMAKE_DIR: &str = "cmake";
const MINGW_X86_64_DIR: &str = "mingw64";
const MINGW_I686_DIR: &str = "mingw32";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser, Debug)]
#[command(version, about = "CMake Build System by Rust", long_about = None)]
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
        // 获取当前工作目录的 PathBuf
        let current_dir = env::current_dir()
            .expect("无法获取当前目录");

        // 提取目录名称并处理根目录情况
        current_dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                eprintln!("警告：当前目录是根目录，使用默认名称 'root'");
                "root".to_string()
            })
    };

    static ref TOOL_URLS: BTreeMap<&'static str, (&'static str, &'static str)> = {
        let mut map = BTreeMap::new();
        map.insert(
            "cmake",
            (
                "https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6-windows-x86_64.zip",
                CMAKE_DIR,
            ),
        );
        map.insert(
            "x86_64-w64-mingw32-gcc",
            (
                "https://github.com/niXman/mingw-builds-binaries/releases/download/14.2.0-rt_v12-rev1/x86_64-14.2.0-release-posix-seh-ucrt-rt_v12-rev1.7z",
                MINGW_X86_64_DIR,
            ),
        );
        map.insert(
            "i686-w64-mingw32-gcc",
            (
                "https://github.com/niXman/mingw-builds-binaries/releases/download/14.2.0-rt_v12-rev1/i686-14.2.0-release-posix-dwarf-ucrt-rt_v12-rev1.7z",
                MINGW_I686_DIR,
            ),
        );
        map
    };
}

fn main() {
    if let Err(e) = run() {
        eprintln!("❌ 程序执行出错: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    environment_check()?;

//    let config_ide = args.config_ide;
    let arch = args.architecture;
    let build_type = args.build_type;
    let program_name = get_current_dir_name();
    let build_dir = format!("build/{}-{}", build_type, arch);

    let (flags, c_compiler, cxx_compiler) = match arch.as_str() {
        "x64" => ("-m64", "x86_64-w64-mingw32-gcc.exe", "x86_64-w64-mingw32-g++.exe"),
        "x86" => ("-m32", "i686-w64-mingw32-gcc.exe", "i686-w64-mingw32-g++.exe"),
        _ => ("", "", ""), // 默认分支
    };

    let config_command = [
        "-B",
        &build_dir,
        "-S",
        ".",
        "-G",
        "MinGW Makefiles",
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
        &format!("-DCMAKE_BUILD_TYPE={}", build_type),
        &format!("-DCMAKE_C_FLAGS={}", flags),
        &format!("-DCMAKE_CXX_FLAGS={}", flags),
        &format!("-DEXECUTABLE_OUTPUT_PATH=bin"),
        &format!("-DLIBRARY_OUTPUT_PATH=bin"),
        &format!("-DCMAKE_C_COMPILER={}", c_compiler),
        &format!("-DCMAKE_CXX_COMPILER={}", cxx_compiler),
    ];

    let _ = run_command("cmake", &config_command);

    let build_command = [
        "--build",
        &build_dir,
    ];

    let _ = run_command("cmake", &build_command);

    let exe = format!("{}/bin/{}", build_dir, program_name);

    let _ = run_command(&exe, &[]);

    Ok(())
    }

    fn get_current_dir_name() -> &'static str {
        &CURRENT_DIR_NAME
    }

    fn get_url_filename(url: &str) -> Option<String> {
        Url::parse(url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|segments| segments.last())
                    .map(|s| s.to_string())
            })
            .filter(|s| !s.is_empty())
    }

    fn environment_check() -> Result<()> {
        for (tool_name, (url, target_dir)) in TOOL_URLS.iter() {
            if is_tool_available(tool_name) {
                println!("✅ 已安装 {}", tool_name);
                continue;
            } else {
                println!("❌ 未安装 {}, 是否下载？(y/n):", tool_name);
                let mut input = String::new();

                io::stdin().read_line(&mut input).expect("读取输入失败");

                let choice = input.trim();
                if choice == "n" {
                    continue;
                }
            }

            let file_name = get_url_filename(url).ok_or("无法解析URL文件名")?;
            let download_path = Path::new(DOWNLOAD_DIR).join(&file_name);
            let tools_path = Path::new(TOOLS_DIR);

            println!("🛠️  正在配置 {}...", tool_name);
            println!("📥 下载地址: {}", url);

            if !download_path.exists() {
                download(url, None)?;
            }

            let output_dir = tools_path.join(target_dir);
            if output_dir.exists() {
                fs::remove_dir_all(&output_dir)?;
            }

            match Path::new(&file_name).extension().and_then(|s| s.to_str()) {
                Some("zip") => {
                    let temp_dir = tools_path;
                    unzip(&download_path, &temp_dir)?;
                    let filename_without_zip = file_name.strip_suffix(".zip").unwrap_or(&file_name);
                    let old_path = tools_path.join(filename_without_zip);
                    rename_dir(&old_path, &output_dir)?;
                }
                Some("7z") => un7z(&download_path, tools_path)?,
                _ => return Err(format!("不支持的压缩格式: {}", file_name).into()),
            }

            add_tool_to_path(&output_dir.join("bin"))?;
        }
        Ok(())
    }

    fn download(url: &str, filename: Option<&str>) -> Result<PathBuf> {
        let file_name = filename
            .map(|s| s.to_string())
            .or_else(|| {
                Url::parse(url).ok().and_then(|u| {
                    // 在闭包内部完成所有权转换
                    u.path_segments()
                        .and_then(|segments| segments.last())
                        .map(|last| last.to_string()) // 立即转换为String
                })
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "downloaded_file.bin".into());

        let download_dir = Path::new(DOWNLOAD_DIR);
        fs::create_dir_all(download_dir)?;

        let save_path = download_dir.join(file_name);
        let mut response = reqwest::blocking::get(url)?.error_for_status()?;

        let total_size = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|ct| ct.to_str().ok())
            .and_then(|ct| ct.parse::<u64>().ok());

        let pb = ProgressBar::new(total_size.unwrap_or(0)).with_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{bar:40}] {bytes:>7}/{total_bytes:7} {eta:3} ({binary_bytes_per_sec})",
            )?
            .progress_chars("##-"),
        );

        let mut file = BufWriter::new(File::create(&save_path)?);
        let mut downloaded = 0u64;
        let mut chunk_buf = [0u8; 8192 * 8];

        while let Ok(bytes_read) = response.read(&mut chunk_buf) {
            if bytes_read == 0 {
                break;
            }
            file.write_all(&chunk_buf[..bytes_read])?;
            downloaded += bytes_read as u64;
            pb.set_position(downloaded.min(total_size.unwrap_or(downloaded)));
        }

        pb.finish_with_message(format!("✅ 下载完成: {}", save_path.display()));
        Ok(save_path)
    }

    fn unzip(source: &Path, dest: &Path) -> Result<()> {
        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = dest.join(file.mangled_name());

            if file.is_dir() {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    fs::create_dir_all(p)?;
                }
                let mut outfile = File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }
        }
        Ok(())
    }

    fn un7z(source: &Path, dest: &Path) -> Result<()> {
        sevenz_rust::decompress_file(source, dest).map_err(|e| format!("7z解压失败: {}", e))?;
        Ok(())
    }

    fn add_tool_to_path(bin_dir: &Path) -> Result<()> {
        let bin_path = env::current_dir()?.join(bin_dir);
        if !bin_path.exists() {
            return Err(format!("工具目录不存在: {}", bin_path.display()).into());
        }

        let mut paths = env::split_paths(&env::var_os("PATH").unwrap()).collect::<Vec<_>>();
        if !paths.contains(&bin_path) {
            paths.insert(0, bin_path.clone());
            let new_path = env::join_paths(paths)?;
            unsafe {
                env::set_var("PATH", new_path);
            }
        }
        Ok(())
    }

    fn is_tool_available(tool: &str) -> bool {
        Command::new(tool)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn run_command(command: &str, args: &[&str]) -> Result<ExitStatus> {
        println!("🚀 执行命令: {} {}", command, args.join(" "));
    
        Command::new(command)
            .args(args)
            .status()
            .map_err(|e| {
                if e.kind() == ErrorKind::NotFound {
                    format!("命令未找到: {}", command).into()
                } else {
                    e.into()
                }
            })
            .and_then(|status| {
                if status.success() {
                    Ok(status)
                } else {
                    Err(format!("命令执行失败: {}", status).into())
                }
            })
    }

    fn rename_dir(source: &Path, target: &Path) -> std::io::Result<()> {
        match fs::rename(source, target) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // 先删除已存在的目标目录
                fs::remove_dir_all(target)?;
                fs::rename(source, target)
            }
            Err(e) => Err(e),
        }
    }
