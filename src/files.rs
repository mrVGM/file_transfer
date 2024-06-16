use crate::ScopedRoutine;

use std::{collections::VecDeque, io::{Read, Write}, mem::size_of, str::FromStr, sync::Arc};

use tokio::sync::{Mutex, Semaphore};

const MAX_CHUNK_SIZE: u32 = 8 * 1024 * 1024;

pub struct FileChunk {
    pub offset: u64,
    pub size: u32,
    pub data: Vec<u8>
}

impl FileChunk {
    pub fn new() -> Self {
        let data: Vec<u8> = vec![0; MAX_CHUNK_SIZE as usize];

        FileChunk {
            offset: 0,
            size: 0,
            data: data
        }
    }

    pub fn pack(&self) -> Vec<u8> {
        let size = size_of::<u64>() + size_of::<u32>() + self.size as usize;
        let mut res: Vec<u8> = vec![0; size];
        
        let offset_index = 0;
        let size_index = size_of::<u64>();
        let data_index = size_index + size_of::<u32>();

        let bytes = self.offset.to_be_bytes();

        let mut offset = &mut res[..offset_index + size_of::<u64>()];
        offset.clone_from_slice(&self.offset.to_be_bytes());

        let mut size = &mut res[size_index..size_index + size_of::<u32>()];
        size.clone_from_slice(&self.size.to_be_bytes());

        let data = &mut res[data_index..];
        data.clone_from_slice(&self.data[..self.size as usize]);
        res
    }
}


pub struct FileReader {
    path: std::path::PathBuf,
    size: u64,
    payload: Arc<(Mutex<std::fs::File>, (Semaphore, Semaphore), Mutex<VecDeque<FileChunk>>)>,
    read_routine: ScopedRoutine
}

impl FileReader {
    pub fn new(path: &str) -> Self {
        let path = std::path::PathBuf::from(path);
        let file_handle = std::fs::File::open(&path).unwrap();
        let meta = file_handle.metadata().unwrap();
        let file_size = meta.len();

        let payload = Arc::new((
            Mutex::new(file_handle),
            (Semaphore::new(10), Semaphore::new(0)),
            Mutex::new(VecDeque::<FileChunk>::new())
        ));
        let payload_clone = payload.clone();

        
        let read_routine = tokio::spawn(async move {
            let (file, (push_semaphore, pop_semaphore), data) = &*payload;

            let mut read: u64 = 0;

            while read < file_size {
                let permit = push_semaphore.acquire().await.unwrap();
                permit.forget();

                let mut chunk = FileChunk::new();
                chunk.offset = read;
                
                {
                    let file = &mut *file.lock().await;
                    let buff = &mut chunk.data[..];
                    let bytes_read = file.read(buff).unwrap();
                    chunk.size = bytes_read as u32;
                    read += bytes_read as u64;
                }

                let data = &mut *data.lock().await;
                data.push_back(chunk);
                pop_semaphore.add_permits(1);
            }

            pop_semaphore.add_permits(1);
        });

        FileReader {
            path: path,
            size: file_size,
            payload: payload_clone,
            read_routine: ScopedRoutine::new(read_routine)
        }
    }

    pub fn get_size(&self) -> u64 {
        self.size
    }

    pub async fn get_chunk(&self) -> Option<FileChunk> {
        let (_, (push_semaphore, pop_semaphore), data) = &*self.payload;
        let permit = pop_semaphore.acquire().await.unwrap();
        permit.forget();

        let data = &mut *data.lock().await;
        let chunk = data.pop_front();

        if let None = chunk {
            pop_semaphore.add_permits(1);
        }
        else {
            push_semaphore.add_permits(1);
        }

        chunk
    }
}


pub struct FileWriter {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
    size: u64,
    payload: Arc<((Semaphore, Mutex<u32>), Mutex<Vec<FileChunk>>)>,
}

impl FileWriter {
    pub fn new(root: &str, path: &str, size: u64) -> Self {
        let root = std::path::PathBuf::from_str(root).unwrap();
        let path = std::path::PathBuf::from_str(path).unwrap();
        
        let payload = Arc::new((
            (Semaphore::new(0), Mutex::<u32>::new(0)),
            Mutex::<Vec<FileChunk>>::new(vec![])
        ));

        FileWriter {
            root: root,
            path: path,
            size: size,
            payload: payload
        }
    }

    pub fn get_size(&self) -> u64 {
        self.size
    }

    pub async fn push(&self, chunk: FileChunk) {
        let ((semaphore, reminder), chunks) = &*self.payload;
        
        let chunks = &mut *chunks.lock().await;
        let mut index = chunks.len();

        for i in 0..chunks.len() {
            let cur = &chunks[i];

            if chunk.offset < cur.offset {
                index = i;
                break;
            }
        }
        chunks.insert(index, chunk);

        let reminder = &mut *reminder.lock().await;
        semaphore.add_permits(*reminder as usize + 1);
        *reminder = 0;
    }

    pub async fn write(&self) {
        let ((semaphore, reminder), chunks) = &*self.payload;

        let absolute_path = self.root.join(&self.path);        
        let mut paths = vec![self.path.clone()];
        loop {
            if let Some(parent) = paths.last().unwrap().parent() {
                paths.push(parent.to_path_buf());
            }
            else {
                break;
            }
        }
        paths.reverse();

        for d in &paths[0 .. paths.len() - 1] {
            let cur = self.root.join(d);
            if !cur.exists() {
                std::fs::create_dir(cur);
            }
        }
        
        let mut file = std::fs::File::create(absolute_path).unwrap();
        

        let mut written = 0;
        while written < self.size {
            let permit = semaphore.acquire().await.unwrap();
            permit.forget();

            let chunk = {

                let chunks = &mut *chunks.lock().await;
            
                if chunks[0].offset > written {
                    let reminder = &mut *reminder.lock().await;
                    *reminder += 1;
                    None
                }
                else {
                    Some(chunks.pop().unwrap())
                }
            };

            if let Some(chunk) = chunk {
                let buff = &chunk.data[..chunk.size as usize];
                let bytes_written = file.write(buff).unwrap();
                written += bytes_written as u64;
            }
        }
    }
}
