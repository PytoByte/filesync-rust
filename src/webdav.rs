use std::{cmp::Ordering, collections::HashMap, fs::Metadata, path::Path};

use reqwest_dav::{Auth, Client, ClientBuilder, Depth, list_cmd::{ListEntity, ListFile}};
use tokio::{fs::{self, File}, io::AsyncWriteExt};
use chrono::{DateTime, Utc};
use iced::futures::{SinkExt, channel::mpsc};
use anyhow::{Result, anyhow};

use crate::{SyncState, Message};

const METADATA_FILENAME: &str = ".syncmetadata";

#[derive(serde::Serialize, serde::Deserialize, Default, Debug, Clone)]
struct SyncMetadata {
    files: HashMap<String, DateTime<Utc>>
}

struct WebDavWorker {
    client: Client,
    output: mpsc::Sender<Message>
}

// SYNCHRONIZE FILES
pub async fn run_sync(output: mpsc::Sender<Message>, host: String, login: String, password: String, pairs: Vec<(String, String)>) {
    let mut output = output;

    let client = match ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Basic(login, password))
        .build() {
            Ok(client) => { client }
            Err(..) => {
                let _ = output.send(Message::ShowError(String::from("Can't build client"))).await;
                let _ = output.send(Message::StopSynchronize).await;
                return;
            }
    };

    let mut worker = WebDavWorker {
        client,
        output
    };

    if !check_connection(&worker).await {
        let _ = worker.output.send(Message::ShowError(String::from("Can't open connection"))).await;
        let _ = worker.output.send(Message::StopSynchronize).await;
        return;
    }

    let syncmetadata = load_metadata(&worker).await.ok();

    if let Err(e) = synchronize_files(&mut worker, &pairs, &syncmetadata).await {
        let _ = worker.output.send(Message::ShowError(e.to_string())).await;
        let _ = worker.output.send(Message::StopSynchronize).await;
        return;
    }

    if let Err(e) = save_and_upload_metadata(&worker, &pairs, syncmetadata).await {
        let _ = worker.output.send(Message::ShowError(e.to_string())).await;
    }
    
    let _ = worker.output.send(Message::StopSynchronize).await;
}

async fn synchronize_files(worker: &mut WebDavWorker, pairs: &Vec<(String, String)>, syncmetadata: &Option<SyncMetadata>) -> Result<()> {
    for (key, value) in pairs.iter() {
        if let Err(e) = synchronize_file(worker, key, value, syncmetadata).await {
            worker.output.send(Message::ShowError(e.to_string())).await?;
        }
    }
    
    Ok(())
}

async fn synchronize_file(worker: &mut WebDavWorker, local_path: &str, server_path: &str, syncmetadata: &Option<SyncMetadata>) -> Result<()> {
    if is_local_file_exist(local_path).await && is_remote_file_exist(worker, server_path).await {
        match compare_modified_time(worker, local_path, server_path, syncmetadata).await? {
            Ordering::Greater => {
                upload_file(worker, local_path, server_path).await?;
                worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
                return Ok(());
            },
            Ordering::Less => {
                download_file(worker, local_path, server_path).await?;
                worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
                return Ok(());
            },
            _ => {
                worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
                return Ok(());
            }
        }
    } else if is_local_file_exist(local_path).await && !is_remote_file_exist(worker, server_path).await {
        upload_file(worker, local_path, server_path).await?;
        worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
        return Ok(());
    } else if !is_local_file_exist(local_path).await && is_remote_file_exist(worker, server_path).await {
        if is_download_possible(local_path).await {
            download_file(worker, local_path, server_path).await?;
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
            return Ok(());
        } else {
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::CantSynchronize)).await?;
            return Err(anyhow!("Not all dirs in path exist {}", local_path))
        }
    }

    worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::CantSynchronize)).await?;
    Err(anyhow!("Both file don't exist {} <=> {}", local_path, server_path))
}


// CHECK FOR SYNCHRONIZATION AVAIABLE
pub async fn check_sync(output: mpsc::Sender<Message>, host: String, login: String, password: String, pairs: Vec<(String, String)>) {
    let mut output = output;

    let client = match ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Basic(login, password))
        .build() {
            Ok(client) => { client }
            Err(..) => {
                let _ = output.send(Message::ShowError(String::from("Can't build client"))).await;
                let _ = output.send(Message::StopSynchronize).await;
                return;
            }
    };

    let mut worker = WebDavWorker {
        client,
        output
    };

    if !check_connection(&worker).await {
        let _ = worker.output.send(Message::ShowError(String::from("Can't open connection"))).await;
        let _ = worker.output.send(Message::StopSynchronizeCheck).await;
        return;
    }

    let syncmetadata = load_metadata(&worker).await.ok();
    
    if let Err(e) = synchronize_files_check(&mut worker, &pairs, &syncmetadata).await {
        let _ = worker.output.send(Message::ShowError(e.to_string())).await;
    }
    
    let _ = worker.output.send(Message::StopSynchronizeCheck).await;
}

async fn synchronize_files_check(worker: &mut WebDavWorker, pairs: &Vec<(String, String)>, syncmetadata: &Option<SyncMetadata>) -> Result<()> {
    for (key, value) in pairs.iter() {
        if let Err(e) = synchronize_file_check(worker, key, value, syncmetadata).await {
            worker.output.send(Message::ShowError(e.to_string())).await?;
        }
    }
    Ok(())
}

async fn synchronize_file_check(worker: &mut WebDavWorker, local_path: &str, server_path: &str, syncmetadata: &Option<SyncMetadata>) -> Result<()> {
    if is_local_file_exist(local_path).await && is_remote_file_exist(worker, server_path).await {
        match compare_modified_time(worker, local_path, server_path, syncmetadata).await? {
            Ordering::Greater => {
                worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedServer)).await?;
                return Ok(());
            },
            Ordering::Less => {
                worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedDevice)).await?;
                return Ok(());
            },
            _ => {
                worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
                return Ok(());
            }
        }
    } else if is_local_file_exist(local_path).await && !is_remote_file_exist(worker, server_path).await {
        worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedServer)).await?;
        return Ok(());
    } else if !is_local_file_exist(local_path).await && is_remote_file_exist(worker, server_path).await {
        if is_download_possible(local_path).await {
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedDevice)).await?;
            return Ok(());
        } else {
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::CantSynchronize)).await?;
            return Err(anyhow!("Not all dirs in path exist {}", local_path))
        }
    }
    worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::CantSynchronize)).await?;
    Err(anyhow!("Both file don't exist {} <=> {}", local_path, server_path))
}


// FUNCTIONS FOR SAVING REMOTE FILES METADATA
async fn save_and_upload_metadata(
    worker: &WebDavWorker,
    pairs: &[(String, String)],
    syncmetadata: Option<SyncMetadata>,
) -> Result<()> {
    let mut syncmetadata = syncmetadata.unwrap_or_default();

    for (local_path, server_path) in pairs {
        if let Ok(file_metadata) = get_local_file_info(local_path).await {
            if let Ok(modified) = file_metadata.modified() {
                let datetime: DateTime<Utc> = modified.into();
                syncmetadata.files.insert(server_path.clone(), datetime);
            }
        }
    }

    if let Ok(data) = postcard::to_allocvec(&syncmetadata) {
        let temp_path = std::env::temp_dir().join(METADATA_FILENAME);
        if let Ok(mut file) = File::create(&temp_path).await {
            file.write_all(&data).await?;
            upload_file(worker, temp_path.to_str().unwrap(), METADATA_FILENAME).await?;
            fs::remove_file(&temp_path).await?;
        }
    }

    Ok(())
}

async fn load_metadata(worker: &WebDavWorker) -> Result<SyncMetadata> {
    let temp_path = std::env::temp_dir().join(METADATA_FILENAME);
    
    download_file(worker, temp_path.to_str().unwrap(), METADATA_FILENAME).await?;

    let data = fs::read(&temp_path)
        .await
        .and_then(|data| { let _ = std::fs::remove_file(&temp_path); Ok(data) })?;

    Ok(postcard::from_bytes::<SyncMetadata>(&data)?)
}


// DOWNLOAD AND UPLOAD FILES
async fn download_file(worker: &WebDavWorker, local_path: &str, server_path: &str) -> Result<()> {
    let response = worker.client.get(server_path).await?;

    if response.status().is_success() {
        let bytes = response.bytes().await?;
        let mut file = File::create(local_path).await?;
        file.write_all(&bytes).await?;
    } else {
        return Err(anyhow!("Download {} request unsuccess. Code: {}", server_path, response.status()));
    }
    
    Ok(())
}

async fn ensure_remote_directories(worker: &WebDavWorker, server_path: &str) -> Result<()> {
    let dir_path = Path::new(server_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    if dir_path.is_empty() || dir_path == "/" {
        return Ok(());
    }

    let parts: Vec<&str> = dir_path.trim_start_matches('/').split('/').collect();
    let mut current_path = String::from("/");

    for part in parts {
        if part.is_empty() {
            continue;
        }

        current_path.push_str(part);
        current_path.push('/');
        
        let response = worker.client.mkcol_raw(&current_path).await?;

        if response.status() != 405 && response.status() != 201 {
            return Err(anyhow!("Unexpected status while making new remote dirs {}", response.status()));
        }
    }

    Ok(())
}

async fn upload_file(worker: &WebDavWorker, local_path: &str, server_path: &str) -> Result<()> {
    ensure_remote_directories(worker, server_path).await?;

    let content = tokio::fs::read(local_path).await?;
    worker.client.put(server_path, content).await?;
    
    Ok(())
}


// OTHER USEFUL FUNCTIONS
async fn check_connection(worker: &WebDavWorker) -> bool {
    worker.client.list("/", Depth::Number(0)).await.is_ok()
}

async fn is_local_file_exist(filepath: &str) -> bool {
    Path::new(filepath).exists()
}

async fn is_remote_file_exist(worker: &WebDavWorker, filepath: &str) -> bool {
    worker.client.list_raw(filepath, Depth::Number(0)).await.unwrap().status() != 404
}

async fn get_remote_file_info(worker: &WebDavWorker, filepath: &str) -> Result<ListFile> {
    let listvec = worker.client.list(filepath, Depth::Number(0)).await?;

    if let Some(ListEntity::File(listfile)) = listvec.first() {
        Ok(listfile.clone())
    } else {
        Err(anyhow!("Remote file {} not found", filepath))
    }
}

async fn get_local_file_info(filepath: &str) -> Result<Metadata> {
    Ok(fs::metadata(Path::new(filepath)).await?)
}

async fn is_download_possible(local_path: &str) -> bool {
    Path::new(local_path).parent().is_some_and(|path| {
        path.exists()
    })
}

async fn compare_modified_time(worker: &WebDavWorker, local_path: &str, server_path: &str, syncmetadata: &Option<SyncMetadata>) -> Result<Ordering> {
    let metadata = get_local_file_info(local_path).await?;

    if let Some(syncmetadata) = syncmetadata {
        if let Some(datetime) = syncmetadata.files.get(server_path) {
            let metadata_dt: DateTime<Utc> = metadata.modified()?.into();
            return Ok(metadata_dt.cmp(&datetime));
        }
    }
    
    let listfile = get_remote_file_info(worker, server_path).await?;
    let metadata_dt: DateTime<Utc> = metadata.modified()?.into();

    return Ok(metadata_dt.cmp(&listfile.last_modified));
}