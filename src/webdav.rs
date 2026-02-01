use std::{cmp::Ordering, collections::HashMap, fs::Metadata, path::Path, sync::Arc};

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
    output: mpsc::Sender<Message>,
    syncmetadata: Option<SyncMetadata>,
    purpose: SyncPurpose
}

#[derive(Hash, Debug, Clone)]
pub enum SyncPurpose {
    Synchronize,
    Check
}

pub async fn run_sync(output: mpsc::Sender<Message>, host: String, login: String, password: String, pairs: Arc<Vec<(String, String)>>, purpose: SyncPurpose) {
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

    if !check_connection(&client).await {
        let _ = output.send(Message::ShowError(String::from("Can't open connection"))).await;
        let _ = output.send(Message::StopSynchronize).await;
        return;
    }

    let syncmetadata = load_metadata(&client).await.ok();
    let mut worker = WebDavWorker {
        client: client,
        output: output,
        syncmetadata: syncmetadata,
        purpose: purpose
    };

    if let Err(e) = synchronize_files(&mut worker, &pairs).await {
        let _ = worker.output.send(Message::ShowError(e.to_string())).await;
        let _ = worker.output.send(Message::StopSynchronize).await;
        return;
    }

    if let SyncPurpose::Synchronize = worker.purpose {
        if let Err(e) = save_and_upload_metadata(&worker.client, &pairs, &mut worker.syncmetadata.take().unwrap_or_default()).await {
            let _ = worker.output.send(Message::ShowError(e.to_string())).await;
        }
    }

    let _ = worker.output.send(Message::StopSynchronize).await;
}

async fn synchronize_files(worker: &mut WebDavWorker, pairs: &Vec<(String, String)>) -> Result<()> {
    for (key, value) in pairs.iter() {
        if let Err(e) = synchronize_file(worker, key, value).await {
            worker.output.send(Message::ShowError(e.to_string())).await?;
        }
    }
    
    Ok(())
}

async fn synchronize_file(worker: &mut WebDavWorker, local_path: &str, remote_path: &str) -> Result<()> {
    if is_local_file_exist(local_path).await && is_remote_file_exist(&worker.client, remote_path).await? {
        match compare_modified_time(worker, local_path, remote_path).await? {
            Ordering::Greater => {
                return sync_through_uploading(worker, local_path, remote_path).await;
            },
            Ordering::Less => {
                return sync_through_downloading(worker, local_path, remote_path).await;
            },
            _ => {
                worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
                return Ok(());
            }
        }
    } else if is_local_file_exist(local_path).await && !is_remote_file_exist(&worker.client, remote_path).await? {
        return sync_through_uploading(worker, local_path, remote_path).await;
    } else if !is_local_file_exist(local_path).await && is_remote_file_exist(&worker.client, remote_path).await? {
        if is_download_possible(local_path).await {
            return sync_through_downloading(worker, local_path, remote_path).await;
        } else {
            return send_sync_impossible(&mut worker.output, local_path, "Not all dirs in path exist").await;
        }
    }
    send_sync_impossible(&mut worker.output, local_path, "Local and remote files don't exist").await
}

// SYNCHRONIZE WAYS
async fn sync_through_downloading(worker: &mut WebDavWorker, local_path: &str, remote_path: &str) -> Result<()> {
    match &worker.purpose {
        SyncPurpose::Synchronize => {
            download_file(&worker.client, local_path, remote_path).await?;
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
        }
        SyncPurpose::Check => {
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedLocal)).await?;
        }
    }
    
    return Ok(());
}

async fn sync_through_uploading(worker: &mut WebDavWorker, local_path: &str, remote_path: &str) -> Result<()> {
    match &worker.purpose {
        SyncPurpose::Synchronize => {
            upload_file(&worker.client, local_path, remote_path).await?;
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await?;
        }
        SyncPurpose::Check => {
            worker.output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedRemote)).await?;
        }
    }
    
    return Ok(());
}

async fn send_sync_impossible(output: &mut mpsc::Sender<Message>, local_path: &str, msg: &str) -> Result<()> {
    let _ = output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::CantSynchronize)).await;
    Err(anyhow!("For file {}: {}", local_path, msg))
}


// FUNCTIONS FOR SAVING REMOTE FILES METADATA
async fn save_and_upload_metadata(
    client: &Client,
    pairs: &[(String, String)],
    syncmetadata: &mut SyncMetadata
) -> Result<()> {
    for (local_path, remote_path) in pairs {
        let file_metadata = get_local_file_info(local_path).await?;
        let modified = file_metadata.modified()?;
        let datetime: DateTime<Utc> = modified.into();
        syncmetadata.files.insert(remote_path.clone(), datetime);
    }

    let data = postcard::to_allocvec(&syncmetadata)?;
    let temp_path = std::env::temp_dir().join(METADATA_FILENAME);
    let mut file = File::create(&temp_path).await?; 
    file.write_all(&data).await?;

    let temp_filepath = match temp_path.to_str() {
        Some(path) => { path }
        None => { return Err(anyhow!("Can't get temp file path")) }
    };

    if let Err(e) =  upload_file(client, temp_filepath, METADATA_FILENAME).await {
        fs::remove_file(&temp_path).await?;
        return Err(e);
    }
    
    fs::remove_file(&temp_path).await?;
    Ok(())
}

async fn load_metadata(client: &Client) -> Result<SyncMetadata> {
    let temp_path = std::env::temp_dir().join(METADATA_FILENAME);

    let temp_filepath = match temp_path.to_str() {
        Some(path) => { path }
        None => { return Err(anyhow!("Can't get temp file path")) }
    };
    
    download_file(client, temp_filepath, METADATA_FILENAME).await?;

    let data = fs::read(&temp_path)
        .await
        .and_then(|data| { let _ = std::fs::remove_file(&temp_path); Ok(data) })?;

    Ok(postcard::from_bytes::<SyncMetadata>(&data)?)
}


// DOWNLOAD AND UPLOAD FILES
async fn download_file(client: &Client, local_path: &str, remote_path: &str) -> Result<()> {
    let response = client.get(remote_path).await?;

    if response.status().is_success() {
        let bytes = response.bytes().await?;
        let mut file = File::create(local_path).await?;
        file.write_all(&bytes).await?;
    } else {
        return Err(anyhow!("Download {} request unsuccess. Code: {}", remote_path, response.status()));
    }
    
    Ok(())
}

async fn ensure_remote_directories(client: &Client, remote_path: &str) -> Result<()> {
    let dir_path = Path::new(remote_path)
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
        
        let response = client.mkcol_raw(&current_path).await?;

        if response.status() != 405 && response.status() != 201 {
            return Err(anyhow!("Unexpected status while making new remote dirs {}", response.status()));
        }
    }

    Ok(())
}

async fn upload_file(client: &Client, local_path: &str, remote_path: &str) -> Result<()> {
    ensure_remote_directories(client, remote_path).await?;

    let content = tokio::fs::read(local_path).await?;
    client.put(remote_path, content).await?;
    
    Ok(())
}


// OTHER USEFUL FUNCTIONS
async fn check_connection(client: &Client) -> bool {
    client.list("/", Depth::Number(0)).await.is_ok()
}

async fn is_local_file_exist(filepath: &str) -> bool {
    Path::new(filepath).exists()
}

async fn is_remote_file_exist(client: &Client, filepath: &str) -> Result<bool> {
    Ok(client.list_raw(filepath, Depth::Number(0)).await?.status() != 404)
}

async fn get_remote_file_info(client: &Client, filepath: &str) -> Result<ListFile> {
    let listvec = client.list(filepath, Depth::Number(0)).await?;

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

async fn compare_modified_time(worker: &WebDavWorker, local_path: &str, remote_path: &str) -> Result<Ordering> {
    let metadata = get_local_file_info(local_path).await?;

    if let Some(syncmetadata) = &worker.syncmetadata {
        if let Some(datetime) = syncmetadata.files.get(remote_path) {
            let metadata_dt: DateTime<Utc> = metadata.modified()?.into();
            return Ok(metadata_dt.cmp(&datetime));
        }
    }
    
    let listfile = get_remote_file_info(&worker.client, remote_path).await?;
    let metadata_dt: DateTime<Utc> = metadata.modified()?.into();

    return Ok(metadata_dt.cmp(&listfile.last_modified));
}