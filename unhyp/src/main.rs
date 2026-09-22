//! `unhyp` — `.hyp`를 푼다.
//!
//! 런처는 이 바이너리가 아니라 `hyp` 라이브러리를 직접 링크한다. 이 CLI는
//! 사람이 확인하고 디버깅하기 위한 것이다.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
unhyp — .hyp 아카이브를 푼다

사용법:
  unhyp [extract] <아카이브.hyp> <대상 디렉터리> [옵션]
  unhyp --list <아카이브.hyp>
  unhyp --info <아카이브.hyp>

옵션:
  -q, --quiet   진행률을 표시하지 않는다
  -h, --help    이 도움말

예:
  unhyp MOTP_0.7.0_windows_x64.hyp \"%USERPROFILE%/HYLauncher/motp\"
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("unhyp: {msg}");
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
    if args.first().map(|s| s.as_str()) == Some("extract") {
        args.remove(0);
    }

    let mut positional: Vec<String> = Vec::new();
    let mut quiet = false;
    let mut list = false;
    let mut info = false;

    for a in &args {
        match a.as_str() {
            "-q" | "--quiet" => quiet = true,
            "--list" | "-l" => list = true,
            "--info" => info = true,
            s if s.starts_with('-') => return Err(format!("모르는 옵션입니다: {s}")),
            s => positional.push(s.to_string()),
        }
    }

    if list || info {
        let archive = positional.first().ok_or("아카이브 경로가 없습니다")?;
        let meta = hyp::read_info(&PathBuf::from(archive)).map_err(|e| e.to_string())?;
        println!("형식 버전  : HYPV{}", meta.format_version);
        println!("엔트리     : {}개", meta.entry_count);
        println!("원본 크기  : {}", human(meta.total_uncompressed));
        if list {
            println!();
            for e in &meta.entries {
                println!("{:>12}  {}{}", human(e.size), e.path, if e.exec { "  *" } else { "" });
            }
        }
        return Ok(());
    }

    if positional.len() != 2 {
        return Err(format!("인자가 2개여야 합니다 (받은 개수: {})\n\n{USAGE}", positional.len()));
    }
    let archive = PathBuf::from(&positional[0]);
    let dest = PathBuf::from(&positional[1]);

    let started = std::time::Instant::now();
    let mut bar = Progress::new(quiet, "해제");
    let summary = hyp::extract(&archive, &dest, |d, t| bar.update(d, t)).map_err(|e| e.to_string())?;
    bar.finish();

    eprintln!(
        "✓ {} — 파일 {}개, {}, {:.1}초",
        dest.display(),
        summary.entry_count,
        human(summary.total_uncompressed),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn human(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", U[i]) }
}

struct Progress {
    quiet: bool,
    label: &'static str,
    last: std::time::Instant,
    drawn: bool,
}

impl Progress {
    fn new(quiet: bool, label: &'static str) -> Self {
        Self { quiet, label, last: std::time::Instant::now(), drawn: false }
    }
    fn update(&mut self, done: u64, total: u64) {
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
    fn finish(&mut self) {
        if self.drawn {
            eprintln!();
        }
    }
}
