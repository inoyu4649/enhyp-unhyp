//! `enhyp` — 디렉터리를 `.hyp`로.
//!
//! 인자 파싱을 손으로 한다. clap을 붙일 만큼 표면이 넓지 않고, 이 바이너리는
//! MOTP의 빌드 스크립트가 호출하는 게 전부다.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
enhyp — 디렉터리를 .hyp 아카이브로 압축한다

사용법:
  enhyp [pack] <디렉터리> <출력.hyp> [옵션]

옵션:
  --exclude <이름>   해당 이름의 파일·디렉터리를 제외한다 (여러 번 쓸 수 있음)
  --no-default-exclude  기본 제외 목록(.git, node_modules 등)을 쓰지 않는다
  --preset <0-9>     XZ 압축 수준 (기본 9 = 최대 압축)
  -q, --quiet        진행률을 표시하지 않는다
  -h, --help         이 도움말

예:
  enhyp dist/staging/windows MOTP_0.7.0_windows_x64.hyp
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("enhyp: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") || args.is_empty() {
        print!("{USAGE}");
        return Ok(());
    }
    // `pack`은 읽기 좋으라고 받아 주는 선택적 동사다.
    if args.first().map(|s| s.as_str()) == Some("pack") {
        args.remove(0);
    }

    let mut positional: Vec<String> = Vec::new();
    let mut opts = hyp::PackOptions::default();
    let mut extra_exclude: Vec<String> = Vec::new();
    let mut clear_default = false;
    let mut quiet = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--exclude" => {
                i += 1;
                let v = args.get(i).ok_or("--exclude 뒤에 이름이 없습니다")?;
                extra_exclude.push(v.clone());
            }
            "--no-default-exclude" => clear_default = true,
            "--preset" => {
                i += 1;
                let v = args.get(i).ok_or("--preset 뒤에 숫자가 없습니다")?;
                opts.preset = v.parse().map_err(|_| format!("--preset 값이 숫자가 아닙니다: {v}"))?;
                if opts.preset > 9 {
                    return Err("--preset 은 0..9 입니다".into());
                }
            }
            "-q" | "--quiet" => quiet = true,
            a if a.starts_with('-') => return Err(format!("모르는 옵션입니다: {a}")),
            a => positional.push(a.to_string()),
        }
        i += 1;
    }

    if positional.len() != 2 {
        return Err(format!("인자가 2개여야 합니다 (받은 개수: {})\n\n{USAGE}", positional.len()));
    }
    let src = PathBuf::from(&positional[0]);
    let dest = PathBuf::from(&positional[1]);

    if !src.is_dir() {
        return Err(format!("디렉터리가 아닙니다: {}", src.display()));
    }
    if clear_default {
        opts.exclude.clear();
    }
    opts.exclude.extend(extra_exclude);

    let started = std::time::Instant::now();
    let mut bar = Progress::new(quiet, "압축");
    let summary = hyp::pack(&src, &dest, &opts, |d, t| bar.update(d, t))
        .map_err(|e| e.to_string())?;
    bar.finish();

    let ratio = if summary.total_uncompressed == 0 {
        0.0
    } else {
        100.0 * summary.compressed_size as f64 / summary.total_uncompressed as f64
    };
    eprintln!(
        "✓ {} — 파일 {}개, {} → {} ({:.1}%), {:.1}초",
        dest.display(),
        summary.entry_count,
        human(summary.total_uncompressed),
        human(summary.compressed_size),
        ratio,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

pub fn human(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", U[i]) }
}

/// 진행률. 터미널을 깜빡이지 않게 100 ms마다만 다시 그린다.
pub struct Progress {
    quiet: bool,
    label: &'static str,
    last: std::time::Instant,
    drawn: bool,
}

impl Progress {
    pub fn new(quiet: bool, label: &'static str) -> Self {
        Self { quiet, label, last: std::time::Instant::now(), drawn: false }
    }
    pub fn update(&mut self, done: u64, total: u64) {
        if self.quiet || total == 0 {
            return;
        }
        let final_tick = done == total;
        if !final_tick && self.last.elapsed().as_millis() < 100 {
            return;
        }
        self.last = std::time::Instant::now();
        self.drawn = true;
        let pct = 100.0 * done as f64 / total as f64;
        eprint!("\r  {} {pct:5.1}%  {} / {}   ", self.label, human(done), human(total));
    }
    pub fn finish(&mut self) {
        if self.drawn {
            eprintln!();
        }
    }
}
