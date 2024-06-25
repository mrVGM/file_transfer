use std::{path::PathBuf, str::FromStr};

pub struct FileData {
    pub relative_path: PathBuf,
    pub size: u64,
}

pub fn get_files() -> Vec<FileData> {
    let cur_dir = get_root_dir();
    let mut to_process = vec![cur_dir.clone()];
    
    let mut res: Vec<PathBuf> = vec![];
    while !to_process.is_empty() {
        let cur = to_process.remove(0);
        
        if cur.is_file() {
            res.push(cur);
            continue;
        }

        if cur.is_dir() {
            let contents = cur.read_dir().unwrap();
            for f in contents {
                let f = f.unwrap();
                to_process.push(f.path());
            }
        }
    }
    let res = res.into_iter().map(|f| {
        let meta = std::fs::metadata(&f).unwrap();

        let p = f.as_path();
        let p = p.strip_prefix(&cur_dir).unwrap();
        FileData {
            relative_path: p.to_path_buf(),
            size: meta.len()
        }
    });
    
    res.collect()
}

pub fn get_root_dir() -> PathBuf {
    let mut root_dir = std::env::current_dir().unwrap();

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let path_str = &*args[1];
        let path = PathBuf::from_str(path_str).unwrap();
        if path.is_absolute() {
            root_dir = path;
        }
        else {
            root_dir = root_dir.join(path);
        }
    }

    root_dir
}