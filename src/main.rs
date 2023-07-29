use std::{
    collections::HashMap,
    fs::{copy, create_dir_all, read_dir, File, ReadDir},
    io::{Read, Result},
    path::{Path, PathBuf},
    process::exit,
    sync::atomic::{AtomicUsize, Ordering},
};

use clap::Parser;
use md5::Context;
use rayon::prelude::*;

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(long, short)]
    root: PathBuf,

    #[arg(default_value = ".", long, short)]
    work_dir: PathBuf,

    #[arg(long, short)]
    out_dir: Option<PathBuf>,

    #[arg(default_value_t = false, long, short = 'R')]
    rename: bool,
}

struct FileIterator {
    dirs: Vec<ReadDir>,
}

impl FileIterator {
    fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            dirs: vec![read_dir(path)?],
        })
    }
}

impl Iterator for FileIterator {
    type Item = Result<PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let dir = self.dirs.last_mut()?;

            if let Some(entry) = dir.next() {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();

                        if path.is_dir() {
                            match read_dir(path) {
                                Ok(dir) => {
                                    self.dirs.push(dir);
                                }
                                Err(error) => return Some(Err(error)),
                            }
                        } else {
                            return Some(Ok(path));
                        }
                    }
                    Err(error) => return Some(Err(error)),
                }
            } else {
                self.dirs.pop();
            }
        }
    }
}

fn hash<P: AsRef<Path>>(path: P) -> Result<(P, String)> {
    let mut file = File::open(&path)?;
    let mut buffer = [0; 4 * 1024];
    let mut context = Context::new();

    loop {
        let read = file.read(&mut buffer)?;

        if read > 0 {
            context.consume(&buffer[..read]);
        } else {
            break;
        }
    }

    let hash = format!("{:x}", context.compute());

    Ok((path, hash))
}

fn main() -> Result<()> {
    let mut args = Args::parse();

    if !args.root.exists() {
        eprintln!("root {:?} doesn't exist", args.root);
        exit(1);
    }

    args.work_dir = args.root.join(args.work_dir);

    if !args.work_dir.exists() {
        eprintln!("work-dir {:?} doesn't exist", args.work_dir);
        exit(1);
    } else if !args.work_dir.starts_with(&args.root) {
        eprintln!(
            "work-dir {:?} is not under root {:?}",
            args.work_dir, args.root
        );

        exit(1);
    }

    let out_dir = args
        .out_dir
        .unwrap_or_else(|| args.work_dir.with_extension("uniq"));

    create_dir_all(&out_dir)?;

    eprintln!("Hashing files...");

    let files = AtomicUsize::new(0);

    let file_hashes = FileIterator::new(&args.root)?
        .par_bridge()
        .map(|path| path.and_then(hash))
        .inspect(|_| {
            let files = files.fetch_add(1, Ordering::Relaxed) + 1;

            if files % 100 == 0 {
                eprintln!("...{files}");
            }
        })
        .collect::<Vec<_>>();

    eprintln!(
        "Hashed {} files and doing the real work now...",
        file_hashes.len()
    );

    let mut hashed_files = HashMap::new();

    for file_hash in file_hashes {
        let (path, hash) = file_hash?;
        hashed_files.entry(hash).or_insert_with(Vec::new).push(path);
    }

    let mut ignored_files = Vec::new();

    for (hash, same_files) in &mut hashed_files {
        same_files.sort();

        let mut working_files = same_files
            .iter()
            .filter(|file| file.starts_with(&args.work_dir));

        if let Some(working_file) = working_files.next() {
            if let Some(existing_file) = same_files
                .iter()
                .find(|file| !file.starts_with(&args.work_dir))
            {
                ignored_files.push(format!(
                    "{} = {}",
                    working_file.strip_prefix(&args.work_dir).unwrap().display(),
                    existing_file.strip_prefix(&args.root).unwrap().display()
                ));
            } else {
                let mut file_name = working_file.file_name().unwrap().to_string_lossy();

                if args.rename {
                    file_name = format!("{hash}_{file_name}").into();
                }

                let mut out_file = PathBuf::new();

                out_file.push(&out_dir);
                out_file.push(file_name.as_ref());

                if !args.rename && out_file.exists() {
                    file_name = format!("{hash}_{file_name}").into();

                    out_file.pop();
                    out_file.push(file_name.as_ref());

                    eprintln!(
                        "File {} is unique, but {} already exists in output directory. Renaming to {}.",
                        working_file.strip_prefix(&args.work_dir).unwrap().display(),
                        working_file.file_name().unwrap().to_string_lossy(),
                        file_name
                    );
                }

                copy(working_file, out_file)?;

                working_files.for_each(|file| {
                    ignored_files.push(format!(
                        "{} = {}",
                        file.strip_prefix(&args.work_dir).unwrap().display(),
                        file_name
                    ));
                });
            }
        }
    }

    ignored_files.sort();

    for ignored_file in ignored_files {
        println!("{ignored_file}");
    }

    eprintln!("... aaaand done :-)");

    Ok(())
}
