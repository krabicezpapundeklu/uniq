use std::{
    collections::HashMap,
    fs::{copy, create_dir_all, read, read_dir},
    io::Result,
    path::{Path, PathBuf},
    process::exit,
};

use clap::Parser;
use md5::compute;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    root: PathBuf,

    #[arg(long)]
    work_dir: PathBuf,

    #[arg(long)]
    out_dir: Option<PathBuf>,

    #[arg(default_value_t = false, long)]
    rename: bool,
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
        .unwrap_or_else(|| args.work_dir.with_extension("out"));

    create_dir_all(&out_dir)?;

    let mut hashed_files = HashMap::new();

    visit_files(&args.root, &mut |file| {
        let body = read(&file)?;
        let hash = format!("{:X}", compute(body));
        let same_files = hashed_files.entry(hash).or_insert_with(Vec::new);

        same_files.push(file);

        Ok(())
    })?;

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
                    "{:?} = {:?}",
                    working_file.strip_prefix(&args.work_dir).unwrap(),
                    existing_file.strip_prefix(&args.root).unwrap()
                ));
            } else {
                let file_name = working_file.file_name().unwrap().to_str().unwrap();
                let mut out_file = PathBuf::new();

                out_file.push(&out_dir);

                if args.rename {
                    out_file.push(format!("{hash}_{file_name}"));
                } else {
                    out_file.push(file_name);
                }

                copy(working_file, out_file)?;

                working_files.for_each(|file| {
                    ignored_files.push(format!(
                        "{:?} = {:?}",
                        file.strip_prefix(&args.work_dir).unwrap(),
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

    Ok(())
}

fn visit_files<F>(path: &Path, f: &mut F) -> Result<()>
where
    F: FnMut(PathBuf) -> Result<()>,
{
    for entry in read_dir(path)? {
        let path = entry?.path();

        if path.is_dir() {
            visit_files(&path, f)?;
        } else {
            f(path)?;
        }
    }

    Ok(())
}
