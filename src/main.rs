use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use once_cell::sync::Lazy;
use reqwest;
use std::{
    env,
    io::{self, Write},
    path::Path,        
    process::Command,
    sync::Arc,
};
use tokio::task::spawn_blocking;
use url::Url;
use zip::ZipArchive;

// 使用 Arc 替代 Mutex 实现线程安全的静态配置
static URLS: Lazy<Arc<Urls>> = Lazy::new(|| {
    Arc::new(Urls {
        x86_64_cmake: "https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6-windows-x86_64.zip"
            .into(),
        i386_cmake: "https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6-windows-i386.zip"
            .into(),
        x86_64_gcc: "https://github.com/niXman/mingw-builds-binaries/releases/download/14.1.0-rt_v12-rev0/x86_64-14.1.0-release-posix-seh-ucrt-rt_v12-rev0.7z"
            .into(),
        i686_gcc: "https://github.com/niXman/mingw-builds-binaries/releases/download/14.1.0-rt_v12-rev0/i686-14.1.0-release-posix-dwarf-ucrt-rt_v12-rev0.7z"
            .into(),
    })
});

#[derive(Debug)]
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
    #[arg(short, long, help = "program name to run")]
    program_name: Option<String>,
}

/// 从 URL 中提取文件名
fn get_filename(url: &str) -> Result<String> {
    Url::parse(url)
        .and_then(|u| {
            u.path_segments()
                .and_then(|s| s.last().map(|s| s.to_owned()))
                .ok_or_else(|| url::ParseError::RelativeUrlWithoutBase)
        })
        .or_else(|_| Ok::<String, url::ParseError>("download.bin".into()))
        .map_err(|e| anyhow::anyhow!("解析 URL 失败: {}", e))
}

/// 获取当前目录名称
fn get_current_dir_name() -> String {
    env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "unknown".into())
}

/// 检查指定程序是否在 PATH 环境变量中
fn is_program_in_path(program: &str) -> bool {
    Command::new(program)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// 交互式下载并自动配置环境
async fn interactive_download(name: &str) -> Result<()> {
    print!("是否现在下载 {}？(y/n): ", name);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    match input.as_str() {
        "y" => {
            let url = match name {
                "x86_64_cmake" => &URLS.x86_64_cmake,
                "i386_cmake" => &URLS.i386_cmake,
                "x86_64_gcc" => &URLS.x86_64_gcc,
                "i686_gcc" => &URLS.i686_gcc,
                _ => anyhow::bail!("未知组件: {}", name),
            };

            let path = download(url).await?;
            let full_path = env::current_dir()?.join(&path);

            // 使用阻塞任务处理压缩文件
            let extract_to = "tools";
            if path.ends_with(".zip") {
                spawn_blocking(move || extract_zip(&full_path, extract_to)).await??;
            } else if path.ends_with(".7z") {
                spawn_blocking(move || extract_7z(&full_path, extract_to)).await??;
            }

            // 更新环境变量 PATH
            let dir_name = Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("解析目录名失败"))?;
        
            let tool_path = Path::new(extract_to)
                .join(dir_name)
                .join("bin")
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("获取工具路径失败: {}", e))?;

            let mut paths = env::split_paths(&env::var_os("PATH").unwrap()).collect::<Vec<_>>();
            paths.insert(0, tool_path);
            unsafe {
                env::set_var("PATH", env::join_paths(paths)?);
                println!("已将添加到 PATH");
            }

            Ok(())
        }
        "n" => {
            println!("用户取消操作");
            Ok(())
        }
        _ => anyhow::bail!("无效输入，请输入 'y' 或 'n'"),
    }
}

/// 带进度条的文件下载
async fn download(url: &str) -> Result<String> {
    let save_dir = "downloads";
    std::fs::create_dir_all(save_dir).context("创建下载目录失败")?;

    let filename = get_filename(url)?;
    let save_path = Path::new(save_dir).join(&filename);

    let client = reqwest::Client::new();
    let response = client.head(url).send().await?;
    let total_size = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|ct_len| ct_len.to_str().ok())
        .and_then(|ct_len| ct_len.parse::<u64>().ok())
        .unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
        .progress_chars("#>-"));

    let mut response = reqwest::get(url).await?;
    let mut file = std::fs::File::create(&save_path)?;
    let mut downloaded = 0;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("下载完成");
    Ok(save_path.to_string_lossy().into_owned())
}

/// 解压 ZIP 文件
fn extract_zip(zip_path: &Path, extract_to: &str) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = Path::new(extract_to).join(file.mangled_name());

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(p)?;
                }
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}

/// 解压 7z 文件
fn extract_7z(sevenz_path: &Path, extract_to: &str) -> Result<()> {
    sevenz_rust::decompress_file(sevenz_path, extract_to)?;
    Ok(())
}

/// 环境检查与自动配置
async fn environment_check() -> Result<()> {
    // 将闭包改为返回 Future 的异步函数
    async fn check_tool(name: &str, component: &str) -> Result<()> {
        if !is_program_in_path(name) {
            println!("{} 未安装或不在 PATH 中", name);
            interactive_download(component).await?;
        }
        Ok(())
    }
    
    // 根据架构检查 CMake
    match std::env::consts::ARCH {
        "x86_64" => check_tool("cmake", "x86_64_cmake").await?,
        "x86" => check_tool("cmake", "i386_cmake").await?,
        arch => anyhow::bail!("不支持的架构: {}", arch),
    }

    // 检查 GCC 工具链
    check_tool("x86_64-w64-mingw32-gcc", "x86_64_gcc").await?;
    check_tool("i686-w64-mingw32-gcc", "i686_gcc").await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // 处理程序名默认值
    let program_name = args.program_name.unwrap_or_else(get_current_dir_name);
    println!("当前项目: {}", program_name);

    environment_check().await?;
    Ok(())
}